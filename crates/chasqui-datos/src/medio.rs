// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Las fotos, notas de voz, vídeos y documentos que manda quien escribe.
//!
//! **Por qué existe este módulo.** Con la Cloud API el número del cliente no
//! vive en ningún teléfono: no hay bandeja de WhatsApp donde mirar. El panel es
//! la única bandeja, así que un medio que no llegue aquí se pierde para el
//! negocio — y hasta el 2026-08-02 se perdían todos, en silencio y sin que nadie
//! pudiera notarlo.
//!
//! **Los bytes van en la base, no en una carpeta.** Está razonado en la
//! migración `0007_medios.sql`: guardar filas hace que la supresión y el
//! respaldo que ya están escritos sigan siendo correctos, y guardar archivos
//! obligaría a añadir dos mecanismos nuevos para que lo siguieran siendo.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::Resultado;

/// Estado de la descarga. Es lo que se le enseña a quien atiende cuando el
/// archivo no está: **«no pude traerlo» y «no hay nada» no son lo mismo**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Anotado, todavía no descargado. Es el estado en que nace: contestarle a
    /// Meta no puede esperar a que baje un vídeo.
    Pendiente,
    Guardado,
    /// Llegó, pero pasaba del tope. **No se descarta callando**: se dice, con el
    /// tamaño, para que quien atiende pueda pedirlo por otra vía.
    DemasiadoGrande,
    Fallido,
}

impl Estado {
    pub fn como_texto(&self) -> &'static str {
        match self {
            Estado::Pendiente => "pendiente",
            Estado::Guardado => "guardado",
            Estado::DemasiadoGrande => "demasiado_grande",
            Estado::Fallido => "fallido",
        }
    }

    pub fn desde_texto(s: &str) -> Option<Estado> {
        match s {
            "pendiente" => Some(Estado::Pendiente),
            "guardado" => Some(Estado::Guardado),
            "demasiado_grande" => Some(Estado::DemasiadoGrande),
            "fallido" => Some(Estado::Fallido),
            _ => None,
        }
    }
}

/// Un medio tal como lo ve el panel. **Sin los bytes**: una lista de veinte
/// mensajes no puede arrastrar veinte fotos en memoria para dibujar la pantalla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ficha {
    pub id: i64,
    pub mensaje_id: i64,
    pub tipo: String,
    pub mime: Option<String>,
    pub nombre_archivo: Option<String>,
    pub estado: Estado,
    pub detalle: Option<String>,
    pub tamano: Option<i64>,
}

/// Anota que llegó un medio. Todavía sin bytes: el webhook tiene que contestarle
/// 200 a Meta en segundos, y la descarga no cabe ahí.
#[allow(clippy::too_many_arguments)]
pub async fn anotar(
    pool: &SqlitePool,
    mensaje_id: i64,
    tipo: &str,
    id_externo: &str,
    mime: Option<&str>,
    nombre_archivo: Option<&str>,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let r = sqlx::query(
        "INSERT INTO medio (mensaje_id, tipo, id_externo, mime, nombre_archivo, estado, creado_en)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pendiente', ?6)
         RETURNING id",
    )
    .bind(mensaje_id)
    .bind(tipo)
    .bind(id_externo)
    .bind(mime)
    .bind(nombre_archivo)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(r.get::<i64, _>("id"))
}

/// Guarda los bytes descargados.
///
/// El `mime` se vuelve a escribir porque el que dice el webhook y el que
/// devuelve la descarga **no siempre coinciden**, y el bueno es el de la
/// descarga: es el que describe lo que de verdad se guardó.
pub async fn guardar(
    pool: &SqlitePool,
    medio_id: i64,
    bytes: &[u8],
    mime: Option<&str>,
) -> Resultado<()> {
    sqlx::query(
        "UPDATE medio
            SET bytes = ?2, tamano = ?3, estado = 'guardado', detalle = NULL,
                mime = COALESCE(?4, mime)
          WHERE id = ?1",
    )
    .bind(medio_id)
    .bind(bytes)
    .bind(bytes.len() as i64)
    .bind(mime)
    .execute(pool)
    .await?;
    Ok(())
}

/// Deja constancia de que el archivo NO está, y de por qué.
///
/// Nunca se borra la fila al fallar: el `id_externo` es el único billete con el
/// que se puede reintentar, y quien atiende tiene derecho a saber que el cliente
/// mandó algo aunque no se haya podido traer.
pub async fn marcar(
    pool: &SqlitePool,
    medio_id: i64,
    estado: Estado,
    detalle: &str,
    tamano: Option<i64>,
) -> Resultado<()> {
    sqlx::query("UPDATE medio SET estado = ?2, detalle = ?3, tamano = COALESCE(?4, tamano) WHERE id = ?1")
        .bind(medio_id)
        .bind(estado.como_texto())
        .bind(detalle)
        .bind(tamano)
        .execute(pool)
        .await?;
    Ok(())
}

/// Las fichas de los medios de una conversación, para pintar el historial.
pub async fn de_conversacion(pool: &SqlitePool, conversacion_id: i64) -> Resultado<Vec<Ficha>> {
    let filas = sqlx::query(
        "SELECT m.id, m.mensaje_id, m.tipo, m.mime, m.nombre_archivo, m.estado, m.detalle, m.tamano
           FROM medio m
           JOIN mensaje ms ON ms.id = m.mensaje_id
          WHERE ms.conversacion_id = ?1
          ORDER BY m.id",
    )
    .bind(conversacion_id)
    .fetch_all(pool)
    .await?;

    Ok(filas
        .into_iter()
        .map(|f| Ficha {
            id: f.get("id"),
            mensaje_id: f.get("mensaje_id"),
            tipo: f.get("tipo"),
            mime: f.get("mime"),
            nombre_archivo: f.get("nombre_archivo"),
            // Un estado que no se sabe leer NO se convierte en «guardado»: eso
            // enseñaría un archivo roto. Se trata como fallo y se dice.
            estado: Estado::desde_texto(&f.get::<String, _>("estado")).unwrap_or(Estado::Fallido),
            detalle: f.get("detalle"),
            tamano: f.get("tamano"),
        })
        .collect())
}

/// Los bytes de un medio, para servirlo. `None` si no está guardado.
pub async fn bytes(pool: &SqlitePool, medio_id: i64) -> Resultado<Option<(Vec<u8>, Option<String>, Option<String>)>> {
    let fila = sqlx::query(
        "SELECT bytes, mime, nombre_archivo FROM medio WHERE id = ?1 AND estado = 'guardado'",
    )
    .bind(medio_id)
    .fetch_optional(pool)
    .await?;

    Ok(fila.and_then(|f| {
        f.get::<Option<Vec<u8>>, _>("bytes")
            .map(|b| (b, f.get("mime"), f.get("nombre_archivo")))
    }))
}

/// Lo que quedó a medias. Para reintentar al arrancar: un corte de red no puede
/// costarle al negocio la foto de un cliente para siempre.
pub async fn pendientes(pool: &SqlitePool, tope: i64) -> Resultado<Vec<(i64, String, i64)>> {
    let filas = sqlx::query(
        "SELECT m.id, m.id_externo, c.cuenta_id
           FROM medio m
           JOIN mensaje ms ON ms.id = m.mensaje_id
           JOIN conversacion c ON c.id = ms.conversacion_id
          WHERE m.estado = 'pendiente'
          ORDER BY m.id
          LIMIT ?1",
    )
    .bind(tope)
    .fetch_all(pool)
    .await?;
    Ok(filas
        .into_iter()
        .map(|f| (f.get("id"), f.get("id_externo"), f.get("cuenta_id")))
        .collect())
}
