// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El historial de mensajes, y con él la idempotencia.
//!
//! Meta reintenta un webhook si no recibe un 200 a tiempo, y «a tiempo» son unos
//! segundos. Si el reintento se atendiera como un mensaje nuevo, quien escribe
//! recibiría la misma respuesta dos y tres veces — el fallo que en producción se
//! ve como «el bot está loco» y en el log no aparece.
//!
//! La defensa es el id que pone Meta, con un índice único en la base. Se elige
//! ahí y no en memoria porque un reinicio del proceso no puede olvidar qué ya
//! contestó.

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::Resultado;

/// Registra un mensaje entrante. Devuelve `false` si ya estaba: es un reintento
/// de Meta y **no hay que volver a atenderlo**.
pub async fn registrar_entrante(
    pool: &SqlitePool,
    conversacion_id: i64,
    id_externo: &str,
    texto: &str,
    momento: DateTime<Utc>,
) -> Resultado<bool> {
    // `ON CONFLICT DO NOTHING` sobre el índice único: la base decide, así que dos
    // reintentos que lleguen a la vez no pueden colarse los dos.
    let r = sqlx::query(
        "INSERT INTO mensaje (conversacion_id, direccion, id_externo, texto, momento)
         VALUES (?1, 'entrante', ?2, ?3, ?4)
         ON CONFLICT (id_externo) WHERE id_externo IS NOT NULL AND direccion = 'entrante'
         DO NOTHING",
    )
    .bind(conversacion_id)
    .bind(id_externo)
    .bind(texto)
    .bind(momento.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(r.rows_affected() == 1)
}

/// Registra lo que se envió, con el acuse que devolvió Meta si lo hubo.
pub async fn registrar_saliente(
    pool: &SqlitePool,
    conversacion_id: i64,
    id_externo: Option<&str>,
    texto: &str,
    momento: DateTime<Utc>,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO mensaje (conversacion_id, direccion, id_externo, texto, momento)
         VALUES (?1, 'saliente', ?2, ?3, ?4)",
    )
    .bind(conversacion_id)
    .bind(id_externo)
    .bind(texto)
    .bind(momento.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Los textos de una conversación, en orden. Para el panel y para atender una
/// solicitud de supresión sabiendo qué hay guardado.
pub async fn de_conversacion(pool: &SqlitePool, id: i64) -> Resultado<Vec<(String, String)>> {
    let filas: Vec<(String, String)> = sqlx::query_as(
        "SELECT direccion, texto FROM mensaje WHERE conversacion_id = ?1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(filas)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::conversacion::abrir_o_continuar;
    use crate::pruebas::base_de_prueba;
    use crate::{cuenta, Boveda};
    use chasqui_dominio::Canal;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    async fn conversacion(pool: &SqlitePool) -> i64 {
        let cta = cuenta::guardar(
            pool,
            &Boveda::nueva("frase de las pruebas, larga y sin sentido"),
            Canal::WhatsApp,
            "PNID1",
            "Ventas",
            "TOKEN",
            t0(),
        )
        .await
        .unwrap();
        abrir_o_continuar(pool, cta, "573001", t0(), 24).await.unwrap().id
    }

    #[tokio::test]
    async fn un_mensaje_nuevo_se_registra() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool).await;
        assert!(registrar_entrante(&pool, c, "wamid.1", "hola", t0()).await.unwrap());
    }

    #[tokio::test]
    async fn el_reintento_de_meta_no_se_atiende_dos_veces() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool).await;

        assert!(registrar_entrante(&pool, c, "wamid.1", "hola", t0()).await.unwrap());
        assert!(
            !registrar_entrante(&pool, c, "wamid.1", "hola", t0()).await.unwrap(),
            "el segundo intento con el mismo id de Meta tiene que decir «ya estaba»"
        );

        let cuantos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mensaje")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cuantos, 1);
    }

    #[tokio::test]
    async fn veinte_reintentos_a_la_vez_dejan_pasar_exactamente_uno() {
        // Directiva 8: concurrencia. Es el escenario real —Meta reintenta en
        // paralelo cuando cree que el webhook se cayó—, y el que un `SELECT` y
        // luego un `INSERT` en dos pasos NO resiste.
        let pool = base_de_prueba().await;
        let c = conversacion(&pool).await;

        let mut tareas = Vec::new();
        for _ in 0..20 {
            let pool = pool.clone();
            tareas.push(tokio::spawn(async move {
                registrar_entrante(&pool, c, "wamid.repetido", "hola", t0()).await.unwrap()
            }));
        }

        let mut aceptados = 0;
        for t in tareas {
            let paso = tokio::time::timeout(std::time::Duration::from_secs(10), t)
                .await
                .expect("una tarea se colgó: hay un interbloqueo en la base")
                .unwrap();
            if paso {
                aceptados += 1;
            }
        }
        assert_eq!(aceptados, 1, "pasaron {aceptados} de 20");
    }

    #[tokio::test]
    async fn los_salientes_no_compiten_por_el_indice_de_los_entrantes() {
        // El índice único es parcial: solo mira los entrantes. Si mirara los dos,
        // enviar dos mensajes sin acuse (id NULL) chocaría… o peor, un saliente
        // con el mismo id que un entrante rechazaría el entrante.
        let pool = base_de_prueba().await;
        let c = conversacion(&pool).await;

        registrar_saliente(&pool, c, None, "primera", t0()).await.unwrap();
        registrar_saliente(&pool, c, None, "segunda", t0()).await.unwrap();
        registrar_saliente(&pool, c, Some("wamid.x"), "tercera", t0()).await.unwrap();
        registrar_saliente(&pool, c, Some("wamid.x"), "cuarta", t0()).await.unwrap();

        assert_eq!(de_conversacion(&pool, c).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn el_historial_sale_en_orden() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool).await;
        registrar_entrante(&pool, c, "wamid.1", "hola", t0()).await.unwrap();
        registrar_saliente(&pool, c, None, "buenas", t0()).await.unwrap();

        let h = de_conversacion(&pool, c).await.unwrap();
        assert_eq!(
            h,
            vec![
                ("entrante".to_string(), "hola".to_string()),
                ("saliente".to_string(), "buenas".to_string())
            ]
        );
    }
}
