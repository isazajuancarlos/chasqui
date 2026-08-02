// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La cola de lo que espera a una persona.
//!
//! Sin esta tabla, «le paso con una persona» es un texto que promete algo que no
//! ocurre. Con ella, el cliente abre el panel y ve quién está esperando y desde
//! cuándo — que es lo que hace que el bot no se apague a las dos semanas.
//!
//! El motivo se guarda porque **es la medida de lo que le falta a la base de
//! conocimiento**: veinte escalados `no_sabe` con el mismo texto son una entrada
//! que hay que escribir, y eso solo se ve si el detalle está guardado.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::atencion::Motivo;

use crate::Resultado;

/// La ranura con la que se guarda cada motivo.
fn ranura(motivo: &Motivo) -> &'static str {
    match motivo {
        Motivo::Peticion => "peticion",
        Motivo::NoSabe(_) => "no_sabe",
        Motivo::NoSoportado(_) => "no_soportado",
        Motivo::FalloTecnico(_) => "fallo_tecnico",
        // Los dos de un flujo van con ranura propia: en la cola, «hay un pedido
        // listo para despachar» y «alguien no supo contestar» son trabajos
        // distintos, y meterlos bajo el mismo nombre los haría indistinguibles
        // justo donde se decide a cuál acudir primero.
        Motivo::PedidoCompleto(_) => "pedido_completo",
        Motivo::PedidoIncompleto(_) => "pedido_incompleto",
    }
}

/// Lo que se guarda como detalle: el texto que no se supo responder, o el tipo
/// de mensaje que no se supo leer.
fn detalle(motivo: &Motivo) -> &str {
    match motivo {
        Motivo::Peticion => "",
        Motivo::NoSabe(t) => t,
        Motivo::NoSoportado(t) => t,
        Motivo::FalloTecnico(t) => t,
        // Aquí va el pedido entero, que es lo que ve quien lo va a despachar.
        Motivo::PedidoCompleto(t) => t,
        Motivo::PedidoIncompleto(t) => t,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pendiente {
    pub id: i64,
    pub conversacion_id: i64,
    pub contacto: String,
    pub motivo: String,
    pub detalle: String,
    pub creado_en: String,
}

/// Encola una conversación para que la atienda una persona.
pub async fn crear(
    pool: &SqlitePool,
    conversacion_id: i64,
    motivo: &Motivo,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO escalado (conversacion_id, motivo, detalle, creado_en)
         VALUES (?1, ?2, ?3, ?4) RETURNING id",
    )
    .bind(conversacion_id)
    .bind(ranura(motivo))
    .bind(detalle(motivo))
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Lo que está esperando, lo que lleva más tiempo primero.
///
/// El orden no es un detalle: quien lleva media hora esperando se atiende antes
/// que quien acaba de escribir, y con el orden al revés el panel enseña siempre
/// lo último y los primeros se quedan para siempre.
pub async fn pendientes(pool: &SqlitePool) -> Resultado<Vec<Pendiente>> {
    let filas = sqlx::query(
        "SELECT e.id, e.conversacion_id, c.contacto, e.motivo, e.detalle, e.creado_en
         FROM escalado e JOIN conversacion c ON c.id = e.conversacion_id
         WHERE e.atendido_en IS NULL
         ORDER BY e.creado_en ASC, e.id ASC",
    )
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            Ok(Pendiente {
                id: f.try_get("id")?,
                conversacion_id: f.try_get("conversacion_id")?,
                contacto: f.try_get("contacto")?,
                motivo: f.try_get("motivo")?,
                detalle: f.try_get("detalle")?,
                creado_en: f.try_get("creado_en")?,
            })
        })
        .collect()
}

/// Marca que una persona ya lo atendió.
pub async fn atender(pool: &SqlitePool, id: i64, ahora: DateTime<Utc>) -> Resultado<()> {
    sqlx::query("UPDATE escalado SET atendido_en = ?1 WHERE id = ?2 AND atendido_en IS NULL")
        .bind(ahora.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ¿Ya hay un escalado sin atender para esta conversación?
///
/// Sirve para no encolar cinco veces a quien escribe cinco mensajes seguidos
/// mientras espera. La cola tiene que reflejar personas esperando, no mensajes.
pub async fn hay_pendiente(pool: &SqlitePool, conversacion_id: i64) -> Resultado<bool> {
    let cuantos: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM escalado WHERE conversacion_id = ?1 AND atendido_en IS NULL",
    )
    .bind(conversacion_id)
    .fetch_one(pool)
    .await?;
    Ok(cuantos > 0)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::conversacion::abrir_o_continuar;
    use crate::pruebas::base_de_prueba;
    use crate::{cuenta, Boveda};
    use chasqui_dominio::Canal;
    use chrono::Duration;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    async fn conversacion(pool: &SqlitePool, contacto: &str) -> i64 {
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
        abrir_o_continuar(pool, cta, contacto, t0(), 24).await.unwrap().id
    }

    #[tokio::test]
    async fn guarda_lo_que_le_preguntaron_cuando_no_supo() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool, "573001").await;
        crear(&pool, c, &Motivo::NoSabe("¿hacen devoluciones?".into()), t0()).await.unwrap();

        let p = pendientes(&pool).await.unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].motivo, "no_sabe");
        assert_eq!(
            p[0].detalle, "¿hacen devoluciones?",
            "sin el detalle no se puede saber qué entrada falta en la base"
        );
        assert_eq!(p[0].contacto, "573001");
    }

    #[tokio::test]
    async fn el_que_lleva_mas_esperando_sale_primero() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool, "573001").await;
        crear(&pool, c, &Motivo::Peticion, t0() + Duration::minutes(30)).await.unwrap();
        crear(&pool, c, &Motivo::Peticion, t0()).await.unwrap();

        let p = pendientes(&pool).await.unwrap();
        assert_eq!(p[0].creado_en, t0().to_rfc3339());
    }

    #[tokio::test]
    async fn lo_atendido_sale_de_la_cola() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool, "573001").await;
        let id = crear(&pool, c, &Motivo::Peticion, t0()).await.unwrap();

        assert!(hay_pendiente(&pool, c).await.unwrap());
        atender(&pool, id, t0() + Duration::minutes(2)).await.unwrap();
        assert!(!hay_pendiente(&pool, c).await.unwrap());
        assert!(pendientes(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn atender_dos_veces_no_reescribe_la_hora_de_la_primera() {
        let pool = base_de_prueba().await;
        let c = conversacion(&pool, "573001").await;
        let id = crear(&pool, c, &Motivo::Peticion, t0()).await.unwrap();

        atender(&pool, id, t0() + Duration::minutes(2)).await.unwrap();
        atender(&pool, id, t0() + Duration::hours(5)).await.unwrap();

        let cuando: String = sqlx::query_scalar("SELECT atendido_en FROM escalado WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cuando, (t0() + Duration::minutes(2)).to_rfc3339());
    }

    #[tokio::test]
    async fn un_escalado_de_peticion_no_guarda_texto_de_la_persona() {
        // Quien pide un asesor no está diciendo nada que haga falta guardar en la
        // cola. Menos dato personal en la cola es menos superficie de habeas data.
        let pool = base_de_prueba().await;
        let c = conversacion(&pool, "573001").await;
        crear(&pool, c, &Motivo::Peticion, t0()).await.unwrap();
        assert_eq!(pendientes(&pool).await.unwrap()[0].detalle, "");
    }
}
