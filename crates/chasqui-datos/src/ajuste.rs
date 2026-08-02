// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Los ajustes del cliente: el guion del bot y lo que lo acompaña.
//!
//! Una tabla clave-valor y no una fila de configuración con veinte columnas: cada
//! ajuste nuevo sería una migración, y el panel tendría que conocer el esquema.
//! Aquí, añadir un ajuste es escribir una clave.
//!
//! **Lo que falta no se rellena.** [`cargar_guion`] no inventa un aviso de
//! privacidad: devuelve el error que dice qué falta y por qué no hay defecto.

use sqlx::SqlitePool;

use chasqui_dominio::Guion;

use crate::{Error, Resultado};

pub const AVISO: &str = "guion.aviso_privacidad";
pub const SIN_RESPUESTA: &str = "guion.sin_respuesta";
pub const ESCALANDO: &str = "guion.escalando";
pub const PALABRAS_ESCALADO: &str = "guion.palabras_escalado";
pub const PALABRAS_CANCELAR: &str = "guion.palabras_cancelar";
pub const CANCELADO: &str = "guion.cancelado";
pub const AVISO_FLUJO: &str = "guion.aviso_flujo";

/// Cuántos días se guardan las conversaciones. **Sin sembrar a propósito**: el
/// plazo lo decide el cliente, que es el responsable del tratamiento. Sin él no
/// se borra nada y el servidor lo dice al arrancar — ver
/// `chasqui_dominio::retencion`.
pub const RETENCION_DIAS: &str = "retencion.dias";

/// Lee un ajuste. `None` si no está.
pub async fn leer(pool: &SqlitePool, clave: &str) -> Resultado<Option<String>> {
    let valor: Option<String> = sqlx::query_scalar("SELECT valor FROM ajuste WHERE clave = ?1")
        .bind(clave)
        .fetch_optional(pool)
        .await?;
    Ok(valor)
}

/// Escribe un ajuste (lo crea o lo reemplaza).
pub async fn poner(pool: &SqlitePool, clave: &str, valor: &str) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO ajuste (clave, valor) VALUES (?1, ?2)
         ON CONFLICT (clave) DO UPDATE SET valor = excluded.valor",
    )
    .bind(clave)
    .bind(valor)
    .execute(pool)
    .await?;
    Ok(())
}

/// Todos los ajustes, para el panel.
pub async fn todos(pool: &SqlitePool) -> Resultado<Vec<(String, String)>> {
    let filas: Vec<(String, String)> =
        sqlx::query_as("SELECT clave, valor FROM ajuste ORDER BY clave")
            .fetch_all(pool)
            .await?;
    Ok(filas)
}

/// Arma el guion con el que atiende el bot.
///
/// Falla si falta el aviso de privacidad o si algún texto quedó en blanco. Es
/// deliberado que el servidor no pueda atender sin esto: el bot trata datos
/// personales desde el primer mensaje, y un aviso que falta no se nota nunca si
/// el bot arranca igual.
pub async fn cargar_guion(pool: &SqlitePool) -> Resultado<Guion> {
    async fn requerido(pool: &SqlitePool, clave: &'static str) -> Resultado<String> {
        leer(pool, clave).await?.ok_or_else(|| Error::AjusteAusente(clave.to_string()))
    }

    /// Una frase por línea, sin las vacías. Una fila en blanco en el panel es de
    /// lo más fácil de dejar por accidente, y una frase vacía casaría con todo.
    fn frases(valor: Option<String>) -> Vec<String> {
        valor
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect()
    }

    let guion = Guion {
        aviso_privacidad: requerido(pool, AVISO).await?,
        sin_respuesta: requerido(pool, SIN_RESPUESTA).await?,
        escalando: requerido(pool, ESCALANDO).await?,
        cancelado: requerido(pool, CANCELADO).await?,
        palabras_escalado: frases(leer(pool, PALABRAS_ESCALADO).await?),
        palabras_cancelar: frases(leer(pool, PALABRAS_CANCELAR).await?),
        // El único que puede faltar sin más: vaciarlo es apagarlo, y es una
        // cortesía, no una regla.
        aviso_flujo: leer(pool, AVISO_FLUJO).await?.unwrap_or_default(),
    };

    // La validación del dominio es la que manda: aquí solo se comprueba que
    // estén las claves, y un valor en blanco pasaría esa comprobación.
    guion.validar()?;
    Ok(guion)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;
    use chasqui_dominio::guion::ErrorGuion;

    #[tokio::test]
    async fn sin_aviso_configurado_el_guion_no_carga() {
        // Estado recién instalado: la migración siembra los textos de cortesía
        // pero NO el aviso. El bot no puede atender así, y tiene que decirlo.
        let pool = base_de_prueba().await;
        let r = cargar_guion(&pool).await;
        assert!(
            matches!(&r, Err(Error::AjusteAusente(c)) if c == AVISO),
            "salió {r:?} en vez del aviso ausente"
        );
    }

    #[tokio::test]
    async fn con_el_aviso_puesto_carga_y_trae_las_palabras_sembradas() {
        let pool = base_de_prueba().await;
        poner(&pool, AVISO, "Sus datos los trata ACME S.A.S.").await.unwrap();

        let g = cargar_guion(&pool).await.unwrap();
        assert_eq!(g.aviso_privacidad, "Sus datos los trata ACME S.A.S.");
        assert!(g.pide_persona("quiero un asesor"), "las palabras sembradas no llegaron");
        assert!(!g.sin_respuesta.is_empty());
    }

    #[tokio::test]
    async fn un_aviso_en_blanco_no_cuenta_como_configurado() {
        // El caso que la comprobación de «existe la clave» dejaría pasar: el
        // cliente borra el texto en el panel y guarda. La fila existe y el aviso
        // legal es una cadena vacía.
        let pool = base_de_prueba().await;
        poner(&pool, AVISO, "   ").await.unwrap();
        assert!(matches!(
            cargar_guion(&pool).await,
            Err(Error::Guion(ErrorGuion::AvisoAusente))
        ));
    }

    #[tokio::test]
    async fn poner_dos_veces_reemplaza_en_vez_de_duplicar() {
        let pool = base_de_prueba().await;
        poner(&pool, AVISO, "primero").await.unwrap();
        poner(&pool, AVISO, "segundo").await.unwrap();
        assert_eq!(leer(&pool, AVISO).await.unwrap().as_deref(), Some("segundo"));
    }

    #[tokio::test]
    async fn sin_palabras_de_escalado_el_guion_sigue_siendo_valido() {
        // Un cliente puede querer que solo se escale cuando el bot no sabe. Eso
        // es inusual, no imposible: la regla no lo rechaza (directiva 21).
        let pool = base_de_prueba().await;
        poner(&pool, AVISO, "aviso").await.unwrap();
        poner(&pool, PALABRAS_ESCALADO, "").await.unwrap();

        let g = cargar_guion(&pool).await.unwrap();
        assert!(g.palabras_escalado.is_empty());
        assert!(!g.pide_persona("asesor"));
    }
}
