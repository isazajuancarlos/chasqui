// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Hablar con el modelo de lenguaje **del cliente**.
//!
//! La clave es suya y la factura le llega a él: ve lo que cuesta y lo controla,
//! igual que con Meta. Y la URL es configurable, así que el cliente que no quiera
//! que sus conversaciones salgan de su máquina apunta a un modelo que corre allí
//! mismo — con lo que la promesa de que los datos no se van deja de tener asterisco.
//!
//! Se habla el dialecto de OpenAI (`/chat/completions`) porque es el que
//! entienden casi todos, incluidos los que se ejecutan en local. No se codifica
//! ningún proveedor por nombre: eso ataría al cliente a uno.
//!
//! Las cotas son las mismas que las de las consultas al sistema del cliente, y
//! por el mismo motivo: **el manejador del webhook está esperando**.

use chasqui_datos::documento::Modelo;
use serde_json::{Value, json};

/// Cuánto se espera al modelo. Más que a una API de pedidos —redactar tarda— y
/// menos de lo que Meta aguanta antes de reintentar el webhook.
const ESPERA: std::time::Duration = std::time::Duration::from_secs(20);

/// Cuánto puede responder. Suficiente para dos o tres frases de mostrador; sin
/// esto, un modelo que se embala escribe una página que WhatsApp parte en cinco
/// mensajes y el cliente paga cada uno.
const MAXIMO_TOKENS: u32 = 300;

#[derive(Debug, thiserror::Error)]
pub enum ErrorModelo {
    #[error("no se pudo hablar con el modelo ({0})")]
    Red(String),

    #[error("el modelo respondió {estado}: {cuerpo}")]
    Rechazado { estado: u16, cuerpo: String },

    #[error(
        "la respuesta del modelo no tiene la forma de la API de OpenAI. ¿La URL apunta a un \
         servidor compatible?"
    )]
    Forma,

    #[error("el modelo devolvió una respuesta vacía")]
    Vacia,
}

/// Le pide al modelo que redacte la respuesta con los fragmentos del cliente.
///
/// Devuelve el texto tal cual. Quien llama decide qué hacer si el modelo dijo
/// que la respuesta no está en los documentos — esa regla vive en el dominio.
pub async fn redactar(
    cliente: &reqwest::Client,
    modelo: &Modelo,
    instrucciones: &str,
    peticion: &str,
) -> Result<String, ErrorModelo> {
    let url = format!("{}/chat/completions", modelo.url.trim_end_matches('/'));

    let cuerpo = json!({
        "model": modelo.nombre,
        "messages": [
            { "role": "system", "content": instrucciones },
            { "role": "user", "content": peticion },
        ],
        // Cero: ante la misma pregunta y los mismos documentos, la misma
        // respuesta. Un bot de atención que contesta distinto cada vez es un bot
        // del que el cliente no se puede fiar — y que no se puede depurar.
        "temperature": 0.0,
        "max_tokens": MAXIMO_TOKENS,
    });

    let mut peticion_http = cliente.post(&url).json(&cuerpo).timeout(ESPERA);
    if let Some(clave) = &modelo.clave {
        peticion_http = peticion_http.bearer_auth(clave);
    }

    let respuesta =
        peticion_http.send().await.map_err(|e| ErrorModelo::Red(e.to_string()))?;
    let estado = respuesta.status();
    let texto = respuesta.text().await.unwrap_or_default();

    if !estado.is_success() {
        return Err(ErrorModelo::Rechazado { estado: estado.as_u16(), cuerpo: texto });
    }

    let json: Value = serde_json::from_str(&texto).map_err(|_| ErrorModelo::Forma)?;
    let contenido = json
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or(ErrorModelo::Forma)?
        .trim()
        .to_string();

    if contenido.is_empty() {
        return Err(ErrorModelo::Vacia);
    }
    Ok(contenido)
}
