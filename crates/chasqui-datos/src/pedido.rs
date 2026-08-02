// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Los pedidos armados por un flujo. **Esto es lo que el negocio vende.**
//!
//! Se guardan aparte del escalado a propósito: un escalado es trabajo que
//! alguien atiende y cierra; un pedido es un hecho del negocio que hay que poder
//! consultar meses después, cuando nadie se acuerda de la conversación.
//!
//! Y el título del flujo se **copia** en vez de referenciarse. Si el cliente
//! renombra o borra el flujo el año que viene, un pedido de hoy tiene que seguir
//! diciendo qué era: un registro que cambia de nombre solo no es un registro.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::{Error, Resultado};

/// Un pedido, tal como se guardó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    pub id: i64,
    pub flujo_id: i64,
    pub titulo: String,
    pub conversacion_id: i64,
    pub contacto: String,
    pub campos: Vec<(String, String)>,
    pub creado_en: DateTime<Utc>,
}

/// Guarda un pedido completo.
pub async fn registrar(
    pool: &SqlitePool,
    flujo_id: i64,
    titulo: &str,
    conversacion_id: i64,
    contacto: &str,
    campos: &[(String, String)],
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let json = serde_json::to_string(campos).unwrap_or_else(|_| "[]".into());
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO pedido (flujo_id, titulo, conversacion_id, contacto, campos, creado_en) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
    )
    .bind(flujo_id)
    .bind(titulo)
    .bind(conversacion_id)
    .bind(contacto)
    .bind(&json)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Los últimos pedidos, para el panel.
pub async fn recientes(pool: &SqlitePool, limite: i64) -> Resultado<Vec<Pedido>> {
    let filas = sqlx::query(
        "SELECT id, flujo_id, titulo, conversacion_id, contacto, campos, creado_en \
         FROM pedido ORDER BY creado_en DESC, id DESC LIMIT ?1",
    )
    .bind(limite)
    .fetch_all(pool)
    .await?;

    filas.into_iter().map(leer).collect()
}

fn leer(fila: sqlx::sqlite::SqliteRow) -> Resultado<Pedido> {
    let id: i64 = fila.try_get("id")?;
    let json: String = fila.try_get("campos")?;
    // Un pedido cuyos campos no se pueden leer NO se devuelve vacío: quien lo
    // mirara vería un pedido sin nada y creería que el cliente no pidió nada.
    let campos: Vec<(String, String)> =
        serde_json::from_str(&json).map_err(|_| Error::ValorInvalido {
            tabla: "pedido",
            id,
            columna: "campos",
            valor: json.clone(),
        })?;
    let creado: String = fila.try_get("creado_en")?;
    let creado_en = DateTime::parse_from_rfc3339(&creado)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| Error::ValorInvalido {
            tabla: "pedido",
            id,
            columna: "creado_en",
            valor: creado,
        })?;

    Ok(Pedido {
        id,
        flujo_id: fila.try_get("flujo_id")?,
        titulo: fila.try_get("titulo")?,
        conversacion_id: fila.try_get("conversacion_id")?,
        contacto: fila.try_get("contacto")?,
        campos,
        creado_en,
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use crate::pruebas::base_de_prueba;

    async fn conversacion(pool: &SqlitePool, contacto: &str) -> i64 {
        sqlx::query(
            "INSERT INTO cuenta_canal (canal, id_externo, nombre, token_cifrado, creada_en) \
             VALUES ('whatsapp', 'PNID-1', 'Ventas', x'00', '2026-08-01T00:00:00+00:00') \
             ON CONFLICT DO NOTHING",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar(
            "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en) \
             VALUES (1, ?1, 'bot', '2026-08-01T00:00:00+00:00', '2026-08-01T00:00:00+00:00') \
             RETURNING id",
        )
        .bind(contacto)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn campos() -> Vec<(String, String)> {
        vec![
            ("producto".into(), "sillas".into()),
            ("cantidad".into(), "3".into()),
            ("direccion".into(), "Calle 5 #3-20".into()),
        ]
    }

    #[tokio::test]
    async fn un_pedido_vuelve_con_sus_campos_en_orden() {
        let pool = base_de_prueba().await;
        let conv = conversacion(&pool, "573001").await;
        registrar(&pool, 3, "Toma de pedido", conv, "573001", &campos(), Utc::now())
            .await
            .unwrap();

        let ps = recientes(&pool, 10).await.unwrap();
        assert_eq!(ps.len(), 1);
        // El orden es el de los pasos, y sin él el pedido se lee al revés.
        assert_eq!(ps[0].campos, campos());
        assert_eq!(ps[0].titulo, "Toma de pedido");
    }

    #[tokio::test]
    async fn el_titulo_sobrevive_a_que_borren_el_flujo() {
        // Un registro que cambia de nombre solo —o que se queda sin él— no es un
        // registro. Por eso el título se copia y no se referencia.
        let pool = base_de_prueba().await;
        let conv = conversacion(&pool, "573001").await;
        registrar(&pool, 99, "Toma de pedido", conv, "573001", &campos(), Utc::now())
            .await
            .unwrap();

        // No hay flujo 99 en la tabla `flujo` y aun así el pedido está entero.
        let ps = recientes(&pool, 10).await.unwrap();
        assert_eq!(ps[0].titulo, "Toma de pedido");
    }

    #[tokio::test]
    async fn borrar_la_conversacion_se_lleva_el_pedido() {
        // ESTA es la prueba de que el derecho de supresión y la retención
        // alcanzan a los pedidos: los dos borran conversaciones, y la cascada de
        // la clave foránea hace el resto. Un borrado propio aquí sería un segundo
        // mecanismo para lo que ya hace el primero — y el que se olvide de
        // actualizar sería el que deja los datos (directiva 1).
        let pool = base_de_prueba().await;
        let conv = conversacion(&pool, "573001").await;
        registrar(&pool, 3, "Pedido", conv, "573001", &campos(), Utc::now()).await.unwrap();

        sqlx::query("DELETE FROM conversacion WHERE id = ?1")
            .bind(conv)
            .execute(&pool)
            .await
            .unwrap();
        assert!(recientes(&pool, 10).await.unwrap().is_empty(), "quedó un pedido huérfano");
    }

    #[tokio::test]
    async fn un_pedido_ilegible_se_denuncia_en_vez_de_salir_vacio() {
        // Devolverlo con cero campos haría creer que el cliente no pidió nada.
        let pool = base_de_prueba().await;
        let conv = conversacion(&pool, "573001").await;
        let id = registrar(&pool, 3, "Pedido", conv, "573001", &campos(), Utc::now())
            .await
            .unwrap();
        sqlx::query("UPDATE pedido SET campos = 'no es json' WHERE id = ?1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(matches!(
            recientes(&pool, 10).await,
            Err(Error::ValorInvalido { tabla: "pedido", columna: "campos", .. })
        ));
    }
}
