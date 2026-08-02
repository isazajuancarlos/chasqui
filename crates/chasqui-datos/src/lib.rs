// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! # chasqui-datos
//!
//! La base de Chasqui: **un archivo SQLite por instancia**, en el servidor del
//! cliente. No hay columna «cliente_id» en ninguna tabla y no debe haberla — el
//! aislamiento entre clientes es que son máquinas distintas, y esa es justo la
//! frase que permite decirle a un cliente que sus conversaciones no salen de su
//! servidor.
//!
//! Aquí vive lo que toca el disco. Las reglas están en `chasqui-dominio`.

#![forbid(unsafe_code)]

pub mod ajuste;
pub mod conocimiento;
pub mod consulta;
pub mod conversacion;
pub mod documento;
pub mod cuenta;
pub mod escalado;
pub mod flujo;
pub mod mensaje;
pub mod pedido;
pub mod respaldo;
pub mod retencion;
pub mod secreto;
pub mod usuario;

use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

pub use secreto::Boveda;

/// Lo que puede salir mal en la capa de datos.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("error de base de datos: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("no se pudo preparar la base: {0}")]
    Migracion(#[from] sqlx::migrate::MigrateError),

    /// Una fila trae en una columna algo que el dominio no reconoce. Se nombra
    /// la fila y el valor: «dato inválido» a secas obliga a buscarlo a mano.
    #[error(
        "la fila {tabla}#{id} trae «{valor}» en la columna «{columna}», que no es un valor \
         válido. La base la escribió una versión distinta de Chasqui o se editó a mano."
    )]
    ValorInvalido { tabla: &'static str, id: i64, columna: &'static str, valor: String },

    #[error(transparent)]
    Guion(#[from] chasqui_dominio::guion::ErrorGuion),

    #[error(transparent)]
    Flujo(#[from] chasqui_dominio::flujo::ErrorFlujo),

    #[error(
        "falta el ajuste «{0}» en la base. No hay valor por defecto: se configura en el panel \
         antes de atender."
    )]
    AjusteAusente(String),

    #[error(
        "no se pudo derivar el hash de la contraseña. Es un fallo del sistema, no del dato: \
         Argon2 solo falla aquí si no hay memoria o si el generador de azar del sistema no \
         responde. NO se guarda nada."
    )]
    ClaveNoHasheable,

    #[error(
        "no se pudo descifrar el token de la cuenta {0}. La causa casi siempre es que \
         CHASQUI_FRASE no es la misma con la que se guardó; también lo sería un archivo de base \
         alterado. Se para: responder con un token que no es el del cliente no es una opción."
    )]
    Descifrado(String),
}

pub type Resultado<T> = Result<T, Error>;

/// Abre la base y la deja lista: migraciones aplicadas, claves foráneas activas.
///
/// `url` es una URL de SQLx (`sqlite://ruta/chasqui.db`). El archivo se crea si
/// no existe: es un producto que se instala en casa del cliente, y exigirle que
/// cree el archivo a mano solo añade un paso que se hace mal.
pub async fn abrir(url: &str) -> Resultado<SqlitePool> {
    let opciones = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        // Sin esto, SQLite acepta escribir filas huérfanas: las referencias de
        // las tablas serían decorativas.
        .foreign_keys(true)
        // WAL deja leer mientras se escribe; un webhook no espera a que el panel
        // termine de listar conversaciones.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        // SQLite admite UN escritor. Con dos webhooks a la vez, el segundo espera
        // en vez de fallar con «database is locked».
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opciones).await?;
    sqlx::migrate!("./migraciones").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Una base en memoria, migrada. `:memory:` con `max_connections(1)` porque
    /// cada conexión nueva a `:memory:` es una base VACÍA distinta: con más de
    /// una, las pruebas verían tablas que no existen a ratos.
    pub(crate) async fn base_de_prueba() -> SqlitePool {
        let opciones = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opciones)
            .await
            .unwrap();
        sqlx::migrate!("./migraciones").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn la_base_se_migra_y_trae_los_textos_de_cortesia_pero_no_el_aviso() {
        let pool = base_de_prueba().await;

        let sembrados: Vec<String> = sqlx::query_scalar("SELECT clave FROM ajuste ORDER BY clave")
            .fetch_all(&pool)
            .await
            .unwrap();

        assert!(sembrados.contains(&"guion.sin_respuesta".to_string()));
        assert!(sembrados.contains(&"guion.escalando".to_string()));
        assert!(
            !sembrados.contains(&"guion.aviso_privacidad".to_string()),
            "el aviso legal NO se siembra: tiene que nombrar al responsable del tratamiento"
        );
    }

    #[tokio::test]
    async fn las_claves_foraneas_muerden() {
        let pool = base_de_prueba().await;
        // Una conversación de una cuenta que no existe tiene que fallar. Si esto
        // pasara, la base admitiría historial huérfano sin decir nada.
        let r = sqlx::query(
            "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en) \
             VALUES (9999, '573001', 'bot', '2026-08-01T00:00:00+00:00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await;
        assert!(r.is_err(), "la clave foránea no está activa");
    }
}
