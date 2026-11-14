// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Las consultas al sistema del cliente, tal como se guardan.
//!
//! La regla —cuál responde, qué dato se saca del texto, cómo se rellena la
//! respuesta— vive en `chasqui-dominio::consulta`. Aquí está el ir y venir a la
//! tabla y, sobre todo, **las cabeceras cifradas**: es donde vive el token del
//! cliente hacia su propio sistema.
//!
//! Las cabeceras se leen aparte y solo cuando se va a llamar. Traerlas con la
//! lista de consultas las metería en memoria cada vez que el bot decide qué
//! hacer, incluso cuando no va a llamar a nadie.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::consulta::{Consulta, Metodo, TipoDato};

use crate::{Boveda, Error, Resultado};

fn disparadores(texto: &str) -> Vec<String> {
    texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Todas las consultas activas.
pub async fn activas(pool: &SqlitePool) -> Resultado<Vec<Consulta>> {
    let filas = sqlx::query(
        "SELECT id, titulo, disparadores, tipo_dato, pregunta, metodo, url, cuerpo,
                plantilla_respuesta
         FROM consulta WHERE activa = 1 ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let id: i64 = f.try_get("id")?;

            let ranura_metodo: String = f.try_get("metodo")?;
            let metodo = Metodo::desde_ranura(&ranura_metodo).ok_or(Error::ValorInvalido {
                tabla: "consulta",
                id,
                columna: "metodo",
                valor: ranura_metodo,
            })?;

            // La columna admite NULL —una consulta puede no necesitar dato—,
            // pero un texto que no es ninguno de los tres tipos SÍ es un error:
            // tratarlo como «sin dato» lanzaría la llamada con la URL a medio
            // rellenar (directiva 20).
            let ranura_tipo: Option<String> = f.try_get("tipo_dato")?;
            let tipo_dato = match ranura_tipo {
                None => None,
                Some(r) => Some(TipoDato::desde_ranura(&r).ok_or(Error::ValorInvalido {
                    tabla: "consulta",
                    id,
                    columna: "tipo_dato",
                    valor: r,
                })?),
            };

            let texto_disparadores: String = f.try_get("disparadores")?;

            Ok(Consulta {
                id,
                titulo: f.try_get("titulo")?,
                disparadores: disparadores(&texto_disparadores),
                tipo_dato,
                pregunta: f.try_get("pregunta")?,
                metodo,
                url: f.try_get("url")?,
                cuerpo: f.try_get("cuerpo")?,
                plantilla_respuesta: f.try_get("plantilla_respuesta")?,
            })
        })
        .collect()
}

/// Las cabeceras de una consulta, descifradas. `None` si no tiene.
///
/// Se piden justo antes de llamar y no se guardan en ningún sitio.
pub async fn cabeceras(pool: &SqlitePool, boveda: &Boveda, id: i64) -> Resultado<Option<String>> {
    let cifradas: Option<Option<Vec<u8>>> =
        sqlx::query_scalar("SELECT cabeceras_cifradas FROM consulta WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await?;

    match cifradas.flatten() {
        None => Ok(None),
        Some(blob) => Ok(Some(boveda.descifrar(&blob, &format!("consulta#{id}"))?)),
    }
}

/// Guarda una consulta nueva.
#[allow(clippy::too_many_arguments)]
pub async fn crear(
    pool: &SqlitePool,
    boveda: &Boveda,
    consulta: &Consulta,
    cabeceras: Option<&str>,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let cifradas = cabeceras.map(|c| boveda.cifrar(c));
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO consulta (titulo, disparadores, tipo_dato, pregunta, metodo, url, cuerpo,
                               cabeceras_cifradas, plantilla_respuesta, activa, creada_en)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10) RETURNING id",
    )
    .bind(&consulta.titulo)
    .bind(consulta.disparadores.join("\n"))
    .bind(consulta.tipo_dato.map(|t| t.ranura()))
    .bind(&consulta.pregunta)
    .bind(consulta.metodo.ranura())
    .bind(&consulta.url)
    .bind(consulta.cuerpo.as_deref())
    .bind(cifradas)
    .bind(&consulta.plantilla_respuesta)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Activa o desactiva una consulta. No se borra: el cliente apaga una
/// integración mientras cambia su API y la vuelve a encender.
pub async fn activar(pool: &SqlitePool, id: i64, activa: bool) -> Resultado<()> {
    sqlx::query("UPDATE consulta SET activa = ?1 WHERE id = ?2")
        .bind(i64::from(activa))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Lo que enseña el panel y la línea de órdenes. **Nunca las cabeceras.**
pub async fn listar(pool: &SqlitePool) -> Resultado<Vec<(i64, String, String, bool, bool)>> {
    let filas = sqlx::query(
        "SELECT id, titulo, url, activa, cabeceras_cifradas IS NOT NULL AS tiene_cabeceras
         FROM consulta ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let activa: i64 = f.try_get("activa")?;
            let con_cabeceras: i64 = f.try_get("tiene_cabeceras")?;
            Ok((
                f.try_get("id")?,
                f.try_get("titulo")?,
                f.try_get("url")?,
                activa != 0,
                con_cabeceras != 0,
            ))
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    fn boveda() -> Boveda {
        Boveda::nueva("frase de las pruebas, larga y sin sentido")
    }

    fn ejemplo() -> Consulta {
        Consulta {
            id: 0,
            titulo: "Estado del pedido".into(),
            disparadores: vec!["mi pedido".into(), "estado del pedido".into()],
            tipo_dato: Some(TipoDato::Numero),
            pregunta: "¿Me confirma el número de pedido?".into(),
            metodo: Metodo::Get,
            url: "https://cliente.example/api/pedidos/{dato}".into(),
            cuerpo: None,
            plantilla_respuesta: "Su pedido está {estado}.".into(),
        }
    }

    #[tokio::test]
    async fn lo_guardado_se_recupera_igual() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), Some("Authorization: Bearer XYZ"), t0())
            .await
            .unwrap();

        let activas = activas(&pool).await.unwrap();
        assert_eq!(activas.len(), 1);
        let c = &activas[0];
        assert_eq!(c.id, id);
        assert_eq!(c.disparadores, vec!["mi pedido", "estado del pedido"]);
        assert_eq!(c.tipo_dato, Some(TipoDato::Numero));
        assert_eq!(c.metodo, Metodo::Get);
        assert_eq!(c.url, "https://cliente.example/api/pedidos/{dato}");
    }

    #[tokio::test]
    async fn las_cabeceras_del_cliente_no_estan_en_claro_en_la_base() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), Some("Authorization: Bearer XYZ"), t0())
            .await
            .unwrap();

        let blob: Vec<u8> = sqlx::query_scalar("SELECT cabeceras_cifradas FROM consulta")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!blob.windows(3).any(|v| v == b"XYZ"), "el token del cliente está en claro");

        assert_eq!(
            cabeceras(&pool, &boveda(), id).await.unwrap().as_deref(),
            Some("Authorization: Bearer XYZ")
        );
    }

    #[tokio::test]
    async fn la_lista_para_el_panel_no_enseña_las_cabeceras() {
        // Lo que no se puede enseñar es el secreto; lo que SÍ hace falta saber es
        // si hay uno configurado, porque «no responde» y «no tiene credenciales»
        // son dos problemas distintos.
        let pool = base_de_prueba().await;
        crear(&pool, &boveda(), &ejemplo(), Some("Authorization: Bearer XYZ"), t0()).await.unwrap();

        let filas = listar(&pool).await.unwrap();
        assert_eq!(filas.len(), 1);
        let (_, titulo, url, activa, tiene_cabeceras) = &filas[0];
        assert_eq!(titulo, "Estado del pedido");
        assert!(activa);
        assert!(tiene_cabeceras);
        assert!(!url.contains("XYZ"));
    }

    #[tokio::test]
    async fn una_consulta_sin_cabeceras_no_es_un_error() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), None, t0()).await.unwrap();
        assert_eq!(cabeceras(&pool, &boveda(), id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn lo_desactivado_no_responde_pero_no_se_pierde() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), None, t0()).await.unwrap();

        activar(&pool, id, false).await.unwrap();
        assert!(activas(&pool).await.unwrap().is_empty());
        assert_eq!(listar(&pool).await.unwrap().len(), 1, "desactivar no es borrar");

        activar(&pool, id, true).await.unwrap();
        assert_eq!(activas(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn un_tipo_de_dato_que_no_existe_se_denuncia_en_vez_de_tratarse_como_sin_dato() {
        // El caso rojo: leerlo como «no necesita dato» lanzaría la llamada con
        // la URL sin rellenar, contra `…/api/pedidos/{dato}` literal.
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), None, t0()).await.unwrap();
        sqlx::query("UPDATE consulta SET tipo_dato = 'telefono' WHERE id = ?1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(matches!(
            activas(&pool).await,
            Err(Error::ValorInvalido { columna: "tipo_dato", .. })
        ));
    }

    #[tokio::test]
    async fn un_metodo_que_no_admitimos_se_denuncia() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, &boveda(), &ejemplo(), None, t0()).await.unwrap();
        sqlx::query("UPDATE consulta SET metodo = 'DELETE' WHERE id = ?1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(matches!(
            activas(&pool).await,
            Err(Error::ValorInvalido { columna: "metodo", .. })
        ));
    }

    #[tokio::test]
    async fn una_consulta_sin_dato_configurado_se_lee_como_tal() {
        let pool = base_de_prueba().await;
        let mut c = ejemplo();
        c.tipo_dato = None;
        crear(&pool, &boveda(), &c, None, t0()).await.unwrap();
        assert_eq!(activas(&pool).await.unwrap()[0].tipo_dato, None);
    }
}
