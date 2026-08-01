// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Abrir, continuar y cerrar conversaciones.
//!
//! Toda la sutileza está en [`abrir_o_continuar`]: decide si el mensaje que llega
//! sigue la conversación de antes o empieza una nueva, y de eso cuelgan el aviso
//! de privacidad y lo que Meta factura.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::conversacion::{Estado, ventana_cerrada};

use crate::{Error, Resultado};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversacion {
    pub id: i64,
    pub cuenta_id: i64,
    pub contacto: String,
    pub estado: Estado,
    pub abierta_en: DateTime<Utc>,
    pub ultimo_en: DateTime<Utc>,
    pub aviso_en: Option<DateTime<Utc>>,
}

impl Conversacion {
    /// ¿Hay que mandar el aviso de privacidad antes de responder?
    pub fn debe_avisar(&self) -> bool {
        self.aviso_en.is_none()
    }
}

fn momento(fila: &sqlx::sqlite::SqliteRow, columna: &'static str, id: i64) -> Resultado<DateTime<Utc>> {
    let texto: String = fila.try_get(columna)?;
    fecha(&texto, columna, id)
}

fn fecha(texto: &str, columna: &'static str, id: i64) -> Resultado<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(texto)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| Error::ValorInvalido {
            tabla: "conversacion",
            id,
            columna,
            valor: texto.to_string(),
        })
}

fn leer(fila: sqlx::sqlite::SqliteRow) -> Resultado<Conversacion> {
    let id: i64 = fila.try_get("id")?;
    let ranura: String = fila.try_get("estado")?;
    let estado = Estado::desde_ranura(&ranura).ok_or(Error::ValorInvalido {
        tabla: "conversacion",
        id,
        columna: "estado",
        valor: ranura,
    })?;
    let aviso: Option<String> = fila.try_get("aviso_en")?;
    Ok(Conversacion {
        id,
        cuenta_id: fila.try_get("cuenta_id")?,
        contacto: fila.try_get("contacto")?,
        estado,
        abierta_en: momento(&fila, "abierta_en", id)?,
        ultimo_en: momento(&fila, "ultimo_en", id)?,
        aviso_en: aviso.map(|t| fecha(&t, "aviso_en", id)).transpose()?,
    })
}

const COLUMNAS: &str =
    "id, cuenta_id, contacto, estado, abierta_en, ultimo_en, aviso_en";

/// La conversación viva de este contacto, o una nueva si la ventana se cerró.
///
/// Devuelve además si hay que avisar. Se hace en una sola operación —y no en un
/// «buscar» y luego un «crear»— porque son la misma decisión: partirla en dos
/// deja el hueco donde dos mensajes seguidos abren dos conversaciones.
pub async fn abrir_o_continuar(
    pool: &SqlitePool,
    cuenta_id: i64,
    contacto: &str,
    ahora: DateTime<Utc>,
    horas_ventana: i64,
) -> Resultado<Conversacion> {
    let viva = sqlx::query(&format!(
        "SELECT {COLUMNAS} FROM conversacion
         WHERE cuenta_id = ?1 AND contacto = ?2 AND cerrada_en IS NULL"
    ))
    .bind(cuenta_id)
    .bind(contacto)
    .fetch_optional(pool)
    .await?;

    if let Some(fila) = viva {
        let c = leer(fila)?;
        if !ventana_cerrada(c.ultimo_en, ahora, horas_ventana) {
            sqlx::query("UPDATE conversacion SET ultimo_en = ?1 WHERE id = ?2")
                .bind(ahora.to_rfc3339())
                .bind(c.id)
                .execute(pool)
                .await?;
            return Ok(Conversacion { ultimo_en: ahora, ..c });
        }
        // La ventana se cerró: esta conversación termina aquí y empieza otra. La
        // vieja se conserva —el historial es lo que se necesita para atender una
        // solicitud de supresión— y libera el índice de «conversación viva».
        sqlx::query("UPDATE conversacion SET cerrada_en = ?1 WHERE id = ?2")
            .bind(ahora.to_rfc3339())
            .bind(c.id)
            .execute(pool)
            .await?;
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en)
         VALUES (?1, ?2, ?3, ?4, ?4)
         RETURNING id",
    )
    .bind(cuenta_id)
    .bind(contacto)
    .bind(Estado::Bot.ranura())
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;

    Ok(Conversacion {
        id,
        cuenta_id,
        contacto: contacto.to_string(),
        estado: Estado::Bot,
        abierta_en: ahora,
        ultimo_en: ahora,
        aviso_en: None,
    })
}

/// Anota que el aviso de privacidad ya salió. Es la prueba, con fecha, de haber
/// avisado; por eso se guarda el momento y no un `1`.
pub async fn marcar_aviso(pool: &SqlitePool, id: i64, ahora: DateTime<Utc>) -> Resultado<()> {
    sqlx::query("UPDATE conversacion SET aviso_en = ?1 WHERE id = ?2 AND aviso_en IS NULL")
        .bind(ahora.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cambia el estado (el bot atiende, espera a una persona, o hay una persona).
pub async fn cambiar_estado(pool: &SqlitePool, id: i64, estado: Estado) -> Resultado<()> {
    sqlx::query("UPDATE conversacion SET estado = ?1 WHERE id = ?2")
        .bind(estado.ranura())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Una conversación por id. Para el panel.
pub async fn por_id(pool: &SqlitePool, id: i64) -> Resultado<Option<Conversacion>> {
    let fila = sqlx::query(&format!("SELECT {COLUMNAS} FROM conversacion WHERE id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    fila.map(leer).transpose()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::cuenta;
    use crate::pruebas::base_de_prueba;
    use crate::Boveda;
    use chasqui_dominio::Canal;
    use chrono::Duration;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    async fn cuenta_de_prueba(pool: &SqlitePool) -> i64 {
        cuenta::guardar(
            pool,
            &Boveda::nueva("frase de las pruebas, larga y sin sentido"),
            Canal::WhatsApp,
            "PNID1",
            "Ventas",
            "TOKEN",
            t0(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn el_primer_mensaje_abre_conversacion_y_pide_aviso() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        assert_eq!(c.estado, Estado::Bot);
        assert!(c.debe_avisar());
    }

    #[tokio::test]
    async fn el_segundo_mensaje_continua_y_ya_no_avisa() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c1 = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        marcar_aviso(&pool, c1.id, t0()).await.unwrap();

        let c2 = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(5), 24)
            .await
            .unwrap();
        assert_eq!(c2.id, c1.id, "no puede abrir otra conversación a los 5 minutos");
        assert!(!c2.debe_avisar());
    }

    #[tokio::test]
    async fn pasada_la_ventana_empieza_otra_conversacion_y_se_vuelve_a_avisar() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c1 = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        marcar_aviso(&pool, c1.id, t0()).await.unwrap();

        let c2 = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::hours(25), 24)
            .await
            .unwrap();
        assert_ne!(c2.id, c1.id);
        assert!(c2.debe_avisar(), "conversación nueva, aviso nuevo");

        // Y la vieja sigue ahí: el historial no se pierde al cerrar.
        let vieja = por_id(&pool, c1.id).await.unwrap().unwrap();
        assert!(vieja.aviso_en.is_some());
    }

    #[tokio::test]
    async fn dos_contactos_no_comparten_conversacion() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let a = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        let b = abrir_o_continuar(&pool, cta, "573002", t0(), 24).await.unwrap();
        assert_ne!(a.id, b.id);
    }

    #[tokio::test]
    async fn el_estado_sobrevive_a_los_mensajes_siguientes() {
        // Si `abrir_o_continuar` reiniciara el estado a `bot`, el bot volvería a
        // hablar en medio de una conversación que ya atiende una persona.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        cambiar_estado(&pool, c.id, Estado::ConHumano).await.unwrap();

        let seguido = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(1), 24)
            .await
            .unwrap();
        assert_eq!(seguido.estado, Estado::ConHumano);
    }

    #[tokio::test]
    async fn marcar_el_aviso_dos_veces_no_cambia_la_fecha_del_primero() {
        // La fecha del aviso es la prueba de CUÁNDO se avisó. Si un reintento la
        // pisara, la prueba diría una hora en la que no se avisó nada.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();

        marcar_aviso(&pool, c.id, t0()).await.unwrap();
        marcar_aviso(&pool, c.id, t0() + Duration::hours(3)).await.unwrap();

        let leida = por_id(&pool, c.id).await.unwrap().unwrap();
        assert_eq!(leida.aviso_en, Some(t0()));
    }

    #[tokio::test]
    async fn cien_mensajes_seguidos_no_abren_cien_conversaciones() {
        // Directiva 8, sobre el mecanismo del que cuelga la facturación de Meta.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let mut ids = std::collections::HashSet::new();
        for i in 0..120 {
            let c = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(i), 24)
                .await
                .unwrap();
            ids.insert(c.id);
        }
        assert_eq!(ids.len(), 1, "se abrieron {} conversaciones", ids.len());

        let filas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(filas, 1);
    }

    #[tokio::test]
    async fn un_estado_que_la_base_no_reconoce_se_denuncia_en_vez_de_suponerse() {
        // Caso rojo de la directiva 20: si esto devolviera `Estado::Bot` por
        // defecto, una base escrita por otra versión pondría al bot a hablar
        // encima de una persona sin un solo error en el log.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();

        sqlx::query("UPDATE conversacion SET estado = 'inventado' WHERE id = ?1")
            .bind(c.id)
            .execute(&pool)
            .await
            .unwrap();

        let r = por_id(&pool, c.id).await;
        assert!(matches!(r, Err(Error::ValorInvalido { columna: "estado", .. })));
    }
}
