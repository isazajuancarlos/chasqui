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
use sqlx::{Row, SqlitePool};

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

/// El id de fila de un entrante ya registrado, buscado por el id de Meta.
///
/// Hace falta para colgarle un medio: `registrar_entrante` dice si era nuevo,
/// que es lo que decide la idempotencia, y cambiar su tipo para devolver además
/// la fila obligaría a tocar todos sus usos por un caso que solo aparece cuando
/// llega una foto. Una consulta más, y solo cuando hay algo que colgar.
pub async fn id_de_entrante(pool: &SqlitePool, id_externo: &str) -> Resultado<Option<i64>> {
    let fila =
        sqlx::query("SELECT id FROM mensaje WHERE id_externo = ?1 AND direccion = 'entrante'")
            .bind(id_externo)
            .fetch_optional(pool)
            .await?;
    Ok(fila.map(|f| f.get::<i64, _>("id")))
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
pub async fn de_conversacion(
    pool: &SqlitePool,
    id: i64,
) -> Resultado<Vec<(i64, String, String, String)>> {
    // El `id` viaja desde el 2026-08-02: sin él no se puede decir a QUÉ mensaje
    // pertenece la foto que se guardó aparte, y el panel enseñaría los archivos
    // sueltos al final en vez de dentro de la conversación.
    let filas: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT id, direccion, texto, momento FROM mensaje WHERE conversacion_id = ?1 ORDER BY id",
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
        let solo_texto: Vec<(String, String)> =
            h.into_iter().map(|(_, d, t, _)| (d, t)).collect();
        assert_eq!(
            solo_texto,
            vec![
                ("entrante".to_string(), "hola".to_string()),
                ("saliente".to_string(), "buenas".to_string())
            ]
        );
    }
}

/// Un mensaje tal como lo lee el widget del canal web.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linea {
    pub id: i64,
    /// `true` si lo escribió quien visita; `false` si lo dijo el bot o una persona.
    pub entrante: bool,
    pub texto: String,
    pub momento: DateTime<Utc>,
}

/// Los mensajes de una conversación posteriores a `desde`.
///
/// El corte va por `id` y no por fecha, y no es un detalle: dos mensajes del
/// mismo segundo son indistinguibles por fecha, así que un sondeo que preguntara
/// «lo posterior a este instante» se saltaría uno o repetiría otro. El `id` es
/// monótono y único.
pub async fn desde(pool: &SqlitePool, conversacion_id: i64, desde: i64) -> Resultado<Vec<Linea>> {
    let filas: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT id, direccion, texto, momento FROM mensaje \
         WHERE conversacion_id = ?1 AND id > ?2 ORDER BY id",
    )
    .bind(conversacion_id)
    .bind(desde)
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|(id, direccion, texto, momento)| {
            // Una dirección que no es ninguna de las dos NO se resuelve a
            // «saliente»: el widget pintaría del lado del bot algo que escribió
            // quien visita, o al revés (directiva 20).
            let entrante = match direccion.as_str() {
                "entrante" => true,
                "saliente" => false,
                otro => {
                    return Err(crate::Error::ValorInvalido {
                        tabla: "mensaje",
                        id,
                        columna: "direccion",
                        valor: otro.to_string(),
                    });
                }
            };
            let momento = DateTime::parse_from_rfc3339(&momento)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| crate::Error::ValorInvalido {
                    tabla: "mensaje",
                    id,
                    columna: "momento",
                    valor: momento,
                })?;
            Ok(Linea { id, entrante, texto, momento })
        })
        .collect()
}
