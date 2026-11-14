// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Los documentos del cliente y el modelo con el que se redactan las respuestas.
//!
//! El troceado y la búsqueda viven en `chasqui-dominio::documentos`; aquí está
//! el ir y venir a la tabla y la clave del modelo, cifrada como todo lo demás
//! que es del cliente.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::documentos::{TAMANO_TROZO, Trozo, trocear};

use crate::{Boveda, Resultado};

/// Guarda o reemplaza un documento por su título.
///
/// Reemplazar y no duplicar: el cliente corrige su catálogo y lo vuelve a
/// cargar. Dos versiones del mismo documento darían dos respuestas distintas a
/// la misma pregunta según cuál recuperara la búsqueda.
pub async fn guardar(
    pool: &SqlitePool,
    titulo: &str,
    texto: &str,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO documento (titulo, texto, activo, creado_en) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT (titulo) DO UPDATE SET texto = excluded.texto, activo = 1
         RETURNING id",
    )
    .bind(titulo)
    .bind(texto)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Todos los documentos activos, ya troceados y listos para buscar.
pub async fn trozos(pool: &SqlitePool) -> Resultado<Vec<Trozo>> {
    let filas: Vec<(String, String)> =
        sqlx::query_as("SELECT titulo, texto FROM documento WHERE activo = 1 ORDER BY id")
            .fetch_all(pool)
            .await?;

    Ok(filas
        .iter()
        .flat_map(|(titulo, texto)| trocear(titulo, texto, TAMANO_TROZO))
        .collect())
}

/// Lo que enseña el panel: qué hay cargado y cuánto ocupa.
pub async fn listar(pool: &SqlitePool) -> Resultado<Vec<(i64, String, i64, bool)>> {
    let filas = sqlx::query(
        "SELECT id, titulo, length(texto) AS tamano, activo FROM documento ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let activo: i64 = f.try_get("activo")?;
            Ok((f.try_get("id")?, f.try_get("titulo")?, f.try_get("tamano")?, activo != 0))
        })
        .collect()
}

pub async fn activar(pool: &SqlitePool, id: i64, activo: bool) -> Resultado<()> {
    sqlx::query("UPDATE documento SET activo = ?1 WHERE id = ?2")
        .bind(i64::from(activo))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// El modelo configurado, con la clave descifrada.
pub struct Modelo {
    pub url: String,
    pub nombre: String,
    pub clave: Option<String>,
}

/// `Debug` que tacha la clave. Ver el de `cuenta::Cuenta`: la protección de «no
/// deriva Debug» dura hasta que alguien añade el derive, y aquí lo que se
/// escaparía al log es la clave del modelo del cliente, que él paga.
impl std::fmt::Debug for Modelo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modelo")
            .field("url", &self.url)
            .field("nombre", &self.nombre)
            .field("clave", &self.clave.as_ref().map(|_| "<oculta>"))
            .finish()
    }
}

/// Configura el modelo. La clave se cifra.
pub async fn guardar_modelo(
    pool: &SqlitePool,
    boveda: &Boveda,
    url: &str,
    nombre: &str,
    clave: Option<&str>,
    ahora: DateTime<Utc>,
) -> Resultado<()> {
    let cifrada = clave.map(|c| boveda.cifrar(c));
    sqlx::query(
        "INSERT INTO modelo (id, url, nombre, clave_cifrada, activo, guardado_en)
         VALUES (1, ?1, ?2, ?3, 1, ?4)
         ON CONFLICT (id) DO UPDATE
             SET url = excluded.url, nombre = excluded.nombre,
                 clave_cifrada = excluded.clave_cifrada, activo = 1,
                 guardado_en = excluded.guardado_en",
    )
    .bind(url.trim_end_matches('/'))
    .bind(nombre)
    .bind(cifrada)
    .bind(ahora.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// El modelo, si está configurado y activo.
pub async fn modelo(pool: &SqlitePool, boveda: &Boveda) -> Resultado<Option<Modelo>> {
    let fila = sqlx::query("SELECT url, nombre, clave_cifrada FROM modelo WHERE id = 1 AND activo = 1")
        .fetch_optional(pool)
        .await?;

    let Some(fila) = fila else { return Ok(None) };
    let cifrada: Option<Vec<u8>> = fila.try_get("clave_cifrada")?;
    let clave = match cifrada {
        None => None,
        Some(blob) => Some(boveda.descifrar(&blob, "modelo")?),
    };

    Ok(Some(Modelo { url: fila.try_get("url")?, nombre: fila.try_get("nombre")?, clave }))
}

/// Lo que el panel puede enseñar del modelo: **nunca la clave**.
pub async fn modelo_visible(pool: &SqlitePool) -> Resultado<Option<(String, String, bool)>> {
    let fila = sqlx::query(
        "SELECT url, nombre, clave_cifrada IS NOT NULL AS con_clave FROM modelo WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    fila.map(|f| {
        let con_clave: i64 = f.try_get("con_clave")?;
        Ok((f.try_get("url")?, f.try_get("nombre")?, con_clave != 0))
    })
    .transpose()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;
    use chasqui_dominio::documentos::recuperar;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    fn boveda() -> Boveda {
        Boveda::nueva("frase de las pruebas, larga y sin sentido")
    }

    #[tokio::test]
    async fn lo_cargado_se_puede_buscar() {
        let pool = base_de_prueba().await;
        guardar(&pool, "catalogo.md", "Envíos\n\nEl envío cuesta 12.000 pesos.", t0())
            .await
            .unwrap();

        let t = trozos(&pool).await.unwrap();
        let encontrados = recuperar("cuánto cuesta el envío", &t, 3);
        assert!(!encontrados.is_empty());
        assert!(encontrados[0].texto.contains("12.000"));
        assert_eq!(encontrados[0].documento, "catalogo.md");
    }

    #[tokio::test]
    async fn recargar_un_documento_lo_reemplaza_en_vez_de_duplicarlo() {
        // Dos versiones del mismo catálogo darían dos respuestas distintas a la
        // misma pregunta, según cuál recuperara la búsqueda primero.
        let pool = base_de_prueba().await;
        guardar(&pool, "catalogo.md", "El envío cuesta 12.000.", t0()).await.unwrap();
        guardar(&pool, "catalogo.md", "El envío cuesta 15.000.", t0()).await.unwrap();

        assert_eq!(listar(&pool).await.unwrap().len(), 1);
        let t = trozos(&pool).await.unwrap();
        assert!(t.iter().all(|x| !x.texto.contains("12.000")));
    }

    #[tokio::test]
    async fn lo_desactivado_no_responde_pero_no_se_pierde() {
        let pool = base_de_prueba().await;
        let id = guardar(&pool, "viejo.md", "Promoción de julio.", t0()).await.unwrap();
        activar(&pool, id, false).await.unwrap();

        assert!(trozos(&pool).await.unwrap().is_empty());
        assert_eq!(listar(&pool).await.unwrap().len(), 1, "desactivar no es borrar");
    }

    #[tokio::test]
    async fn la_clave_del_modelo_no_esta_en_claro_ni_sale_al_panel() {
        let pool = base_de_prueba().await;
        guardar_modelo(&pool, &boveda(), "https://api.ejemplo.com/v1", "gpt-x", Some("sk-SECRETA"), t0())
            .await
            .unwrap();

        let blob: Vec<u8> = sqlx::query_scalar("SELECT clave_cifrada FROM modelo")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!blob.windows(9).any(|v| v == b"sk-SECRET"));

        let m = modelo(&pool, &boveda()).await.unwrap().unwrap();
        assert_eq!(m.clave.as_deref(), Some("sk-SECRETA"));

        let (url, nombre, con_clave) = modelo_visible(&pool).await.unwrap().unwrap();
        assert_eq!(url, "https://api.ejemplo.com/v1");
        assert_eq!(nombre, "gpt-x");
        assert!(con_clave, "el panel tiene que poder decir SI hay clave");
    }

    #[tokio::test]
    async fn sin_modelo_configurado_no_hay_modelo() {
        let pool = base_de_prueba().await;
        assert!(modelo(&pool, &boveda()).await.unwrap().is_none());
        assert!(modelo_visible(&pool).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn un_modelo_en_la_maquina_del_cliente_no_necesita_clave() {
        // Un servidor propio sin autenticación es el caso del cliente que no
        // quiere que sus conversaciones salgan ni al modelo. Es inusual, no
        // imposible: la regla no lo rechaza (directiva 21).
        let pool = base_de_prueba().await;
        guardar_modelo(&pool, &boveda(), "http://127.0.0.1:11434/v1", "llama", None, t0())
            .await
            .unwrap();
        assert!(modelo(&pool, &boveda()).await.unwrap().unwrap().clave.is_none());
    }

    #[tokio::test]
    async fn configurar_dos_veces_no_crea_dos_modelos() {
        let pool = base_de_prueba().await;
        guardar_modelo(&pool, &boveda(), "https://uno.com/v1", "a", None, t0()).await.unwrap();
        guardar_modelo(&pool, &boveda(), "https://dos.com/v1", "b", None, t0()).await.unwrap();

        let cuantos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM modelo")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cuantos, 1);
        assert_eq!(modelo_visible(&pool).await.unwrap().unwrap().1, "b");
    }
}
