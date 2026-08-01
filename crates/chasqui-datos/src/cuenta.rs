// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Las cuentas de canal que atiende esta instancia.
//!
//! Una cuenta es «este número de WhatsApp» o «esta página de Facebook», con el
//! token del cliente para responder por ella. Un webhook trae el id de la cuenta
//! que recibió el mensaje; buscarla aquí es lo que dice con qué credenciales se
//! contesta — y que no exista es motivo para NO contestar, nunca para usar la
//! primera que haya.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::Canal;

use crate::{Boveda, Error, Resultado};

/// Una cuenta de canal con su token ya descifrado y listo para usar.
///
/// El token no se guarda en ninguna estructura de larga vida: se descifra al
/// atender un mensaje y se va con él.
pub struct Cuenta {
    pub id: i64,
    pub canal: Canal,
    pub id_externo: String,
    pub nombre: String,
    pub token: String,
}

/// Da de alta una cuenta (o actualiza su token si ya existía).
///
/// Actualizar en vez de duplicar es lo correcto aquí: el cliente rota el token de
/// Meta y vuelve a ejecutar el alta. Dos filas para el mismo número dejarían una
/// con un token muerto y un 50 % de respuestas fallando.
pub async fn guardar(
    pool: &SqlitePool,
    boveda: &Boveda,
    canal: Canal,
    id_externo: &str,
    nombre: &str,
    token: &str,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let cifrado = boveda.cifrar(token);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO cuenta_canal (canal, id_externo, nombre, token_cifrado, activa, creada_en)
         VALUES (?1, ?2, ?3, ?4, 1, ?5)
         ON CONFLICT (canal, id_externo) DO UPDATE
             SET token_cifrado = excluded.token_cifrado,
                 nombre        = excluded.nombre,
                 activa        = 1
         RETURNING id",
    )
    .bind(canal.ranura())
    .bind(id_externo)
    .bind(nombre)
    .bind(cifrado)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Busca la cuenta activa por el id que trae el webhook.
///
/// `Ok(None)` es «no atendemos esa cuenta»: pasa cuando la app de Meta del
/// cliente tiene suscrito un número que aquí no se dio de alta. Se responde 200 y
/// no se hace nada — un 500 haría que Meta reintentara para siempre.
pub async fn por_id_externo(
    pool: &SqlitePool,
    boveda: &Boveda,
    canal: Canal,
    id_externo: &str,
) -> Resultado<Option<Cuenta>> {
    let fila = sqlx::query(
        "SELECT id, nombre, token_cifrado FROM cuenta_canal
         WHERE canal = ?1 AND id_externo = ?2 AND activa = 1",
    )
    .bind(canal.ranura())
    .bind(id_externo)
    .fetch_optional(pool)
    .await?;

    let Some(fila) = fila else { return Ok(None) };

    let id: i64 = fila.try_get("id")?;
    let cifrado: Vec<u8> = fila.try_get("token_cifrado")?;
    let referencia = format!("{canal}/{id_externo}");
    let token = boveda.descifrar(&cifrado, &referencia)?;

    Ok(Some(Cuenta {
        id,
        canal,
        id_externo: id_externo.to_string(),
        nombre: fila.try_get("nombre")?,
        token,
    }))
}

/// Una cuenta por su id numérico, con el token descifrado.
///
/// La necesita el panel para que una PERSONA pueda responder: el mensaje que
/// escribe un agente sale por el mismo número y con las mismas credenciales que
/// los del bot.
pub async fn por_id(pool: &SqlitePool, boveda: &Boveda, id: i64) -> Resultado<Option<Cuenta>> {
    let fila = sqlx::query(
        "SELECT canal, id_externo, nombre, token_cifrado FROM cuenta_canal WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(fila) = fila else { return Ok(None) };

    let ranura: String = fila.try_get("canal")?;
    let canal = Canal::desde_ranura(&ranura).ok_or(Error::ValorInvalido {
        tabla: "cuenta_canal",
        id,
        columna: "canal",
        valor: ranura,
    })?;
    let id_externo: String = fila.try_get("id_externo")?;
    let cifrado: Vec<u8> = fila.try_get("token_cifrado")?;
    let token = boveda.descifrar(&cifrado, &format!("{canal}/{id_externo}"))?;

    Ok(Some(Cuenta { id, canal, id_externo, nombre: fila.try_get("nombre")?, token }))
}

/// Las cuentas dadas de alta, sin tokens. Para el panel y para el arranque.
pub async fn listar(pool: &SqlitePool) -> Resultado<Vec<(i64, Canal, String, String, bool)>> {
    let filas = sqlx::query("SELECT id, canal, id_externo, nombre, activa FROM cuenta_canal ORDER BY id")
        .fetch_all(pool)
        .await?;

    filas
        .into_iter()
        .map(|f| {
            let id: i64 = f.try_get("id")?;
            let ranura: String = f.try_get("canal")?;
            let canal = Canal::desde_ranura(&ranura).ok_or(Error::ValorInvalido {
                tabla: "cuenta_canal",
                id,
                columna: "canal",
                valor: ranura,
            })?;
            let activa: i64 = f.try_get("activa")?;
            Ok((id, canal, f.try_get("id_externo")?, f.try_get("nombre")?, activa != 0))
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;

    fn ahora() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    fn boveda() -> Boveda {
        Boveda::nueva("frase de despliegue de las pruebas, larga y fea")
    }

    #[tokio::test]
    async fn se_da_de_alta_y_se_recupera_con_el_token_descifrado() {
        let pool = base_de_prueba().await;
        let b = boveda();
        guardar(&pool, &b, Canal::WhatsApp, "PNID1", "Ventas", "TOKEN-1", ahora()).await.unwrap();

        let c = por_id_externo(&pool, &b, Canal::WhatsApp, "PNID1").await.unwrap().unwrap();
        assert_eq!(c.token, "TOKEN-1");
        assert_eq!(c.nombre, "Ventas");
        assert_eq!(c.canal, Canal::WhatsApp);
    }

    #[tokio::test]
    async fn el_token_no_esta_en_claro_en_la_base() {
        let pool = base_de_prueba().await;
        guardar(&pool, &boveda(), Canal::WhatsApp, "PNID1", "Ventas", "TOKEN-1", ahora())
            .await
            .unwrap();

        let blob: Vec<u8> = sqlx::query_scalar("SELECT token_cifrado FROM cuenta_canal")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!blob.windows(7).any(|v| v == b"TOKEN-1"));
    }

    #[tokio::test]
    async fn rotar_el_token_actualiza_la_fila_en_vez_de_duplicarla() {
        let pool = base_de_prueba().await;
        let b = boveda();
        let id1 = guardar(&pool, &b, Canal::WhatsApp, "PNID1", "Ventas", "VIEJO", ahora())
            .await
            .unwrap();
        let id2 = guardar(&pool, &b, Canal::WhatsApp, "PNID1", "Ventas", "NUEVO", ahora())
            .await
            .unwrap();

        assert_eq!(id1, id2, "una rotación de token no puede crear una cuenta nueva");
        let cuantas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cuenta_canal")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cuantas, 1);
        let c = por_id_externo(&pool, &b, Canal::WhatsApp, "PNID1").await.unwrap().unwrap();
        assert_eq!(c.token, "NUEVO");
    }

    #[tokio::test]
    async fn el_mismo_id_en_dos_canales_son_dos_cuentas() {
        // No es un caso teórico: el id de una página de Facebook y el de la
        // cuenta de Instagram conectada pueden coincidir en configuraciones de
        // Meta. Si la llave fuera solo el id, uno pisaría al otro.
        let pool = base_de_prueba().await;
        let b = boveda();
        guardar(&pool, &b, Canal::Messenger, "ID", "Página", "TOK-M", ahora()).await.unwrap();
        guardar(&pool, &b, Canal::Instagram, "ID", "IG", "TOK-I", ahora()).await.unwrap();

        assert_eq!(
            por_id_externo(&pool, &b, Canal::Messenger, "ID").await.unwrap().unwrap().token,
            "TOK-M"
        );
        assert_eq!(
            por_id_externo(&pool, &b, Canal::Instagram, "ID").await.unwrap().unwrap().token,
            "TOK-I"
        );
    }

    #[tokio::test]
    async fn una_cuenta_que_no_existe_no_devuelve_otra() {
        let pool = base_de_prueba().await;
        let b = boveda();
        guardar(&pool, &b, Canal::WhatsApp, "PNID1", "Ventas", "TOKEN-1", ahora()).await.unwrap();
        assert!(por_id_externo(&pool, &b, Canal::WhatsApp, "OTRO").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn con_la_frase_equivocada_se_para_en_vez_de_devolver_basura() {
        let pool = base_de_prueba().await;
        guardar(&pool, &boveda(), Canal::WhatsApp, "PNID1", "Ventas", "TOKEN-1", ahora())
            .await
            .unwrap();

        let otra = Boveda::nueva("la frase que pondría quien restaura un respaldo a medias");
        let r = por_id_externo(&pool, &otra, Canal::WhatsApp, "PNID1").await;
        assert!(matches!(r, Err(Error::Descifrado(_))));
    }
}
