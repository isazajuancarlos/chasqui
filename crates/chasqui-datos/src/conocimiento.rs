// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La base de conocimiento tal como se guarda.
//!
//! La regla de búsqueda vive en `chasqui-dominio::conocimiento`; aquí solo está
//! el ir y venir a la tabla. Los disparadores se guardan **uno por línea** en una
//! columna de texto y no en una tabla aparte: es lo que el cliente edita en un
//! cuadro del panel, y una tabla de disparadores obligaría a una pantalla de alta
//! y baja para lo que en realidad es escribir una lista.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::conocimiento::Entrada;

use crate::Resultado;

/// Parte el texto de la columna en disparadores. Las líneas en blanco se
/// descartan: un disparador vacío casaría con todo.
fn disparadores(texto: &str) -> Vec<String> {
    texto
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Todas las entradas activas, listas para buscar.
pub async fn activas(pool: &SqlitePool) -> Resultado<Vec<Entrada>> {
    let filas = sqlx::query(
        "SELECT id, titulo, respuesta, disparadores FROM conocimiento WHERE activa = 1 ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let texto: String = f.try_get("disparadores")?;
            Ok(Entrada {
                id: f.try_get("id")?,
                titulo: f.try_get("titulo")?,
                respuesta: f.try_get("respuesta")?,
                disparadores: disparadores(&texto),
            })
        })
        .collect()
}

/// Guarda una entrada nueva.
pub async fn crear(
    pool: &SqlitePool,
    titulo: &str,
    respuesta: &str,
    disparadores: &[String],
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conocimiento (titulo, respuesta, disparadores, activa, creada_en)
         VALUES (?1, ?2, ?3, 1, ?4) RETURNING id",
    )
    .bind(titulo)
    .bind(respuesta)
    .bind(disparadores.join("\n"))
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Activa o desactiva una entrada.
///
/// Desactivar y no borrar: el cliente quita una promoción que se acabó y la
/// vuelve a poner el año que viene. Un borrado le costaría escribirla otra vez.
pub async fn activar(pool: &SqlitePool, id: i64, activa: bool) -> Resultado<()> {
    sqlx::query("UPDATE conocimiento SET activa = ?1 WHERE id = ?2")
        .bind(i64::from(activa))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;
    use chasqui_dominio::conocimiento::buscar;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn lo_guardado_se_puede_buscar() {
        let pool = base_de_prueba().await;
        crear(
            &pool,
            "Horario",
            "De 8 a 6.",
            &["horario".into(), "a que hora abren".into()],
            t0(),
        )
        .await
        .unwrap();

        let entradas = activas(&pool).await.unwrap();
        let c = buscar("¿a qué hora abren?", &entradas).expect("debía casar");
        assert_eq!(c.entrada.respuesta, "De 8 a 6.");
    }

    #[tokio::test]
    async fn las_lineas_en_blanco_no_se_vuelven_un_disparador_que_casa_con_todo() {
        // El caso rojo: si una línea vacía sobreviviera, esta entrada respondería
        // a CUALQUIER mensaje y el cliente vería el horario ante toda pregunta.
        let pool = base_de_prueba().await;
        sqlx::query(
            "INSERT INTO conocimiento (titulo, respuesta, disparadores, activa, creada_en)
             VALUES ('Horario', 'De 8 a 6.', 'horario


', 1, '2026-08-01T12:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let entradas = activas(&pool).await.unwrap();
        assert_eq!(entradas[0].disparadores, vec!["horario".to_string()]);
        assert!(buscar("cualquier otra cosa", &entradas).is_none());
    }

    #[tokio::test]
    async fn lo_desactivado_no_responde_pero_no_se_pierde() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, "Promo", "50% de descuento.", &["promocion".into()], t0())
            .await
            .unwrap();

        activar(&pool, id, false).await.unwrap();
        assert!(activas(&pool).await.unwrap().is_empty());

        activar(&pool, id, true).await.unwrap();
        assert_eq!(activas(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn con_la_base_vacia_no_hay_entradas_y_no_es_un_error() {
        let pool = base_de_prueba().await;
        assert!(activas(&pool).await.unwrap().is_empty());
    }
}
