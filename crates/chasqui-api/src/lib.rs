// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! # chasqui-api
//!
//! El servidor: recibe los webhooks de Meta, decide con `chasqui-dominio` y
//! responde por el canal por el que entró.
//!
//! Es una librería además de un binario para que las pruebas puedan levantar el
//! router de verdad y hablarle por HTTP como le habla Meta (directiva 28).

#![forbid(unsafe_code)]

pub mod atender;
pub mod canales;
pub mod config;
pub mod consultas;
pub mod firma;
pub mod servicio;
pub mod webhook;

use axum::Router;
use axum::routing::get;

use servicio::Servicio;

/// El router completo.
pub fn router(servicio: Servicio) -> Router {
    Router::new()
        // Las dos rutas que llama Meta, con el canal en la ruta. Un webhook por
        // canal y no uno solo: así el cuerpo que llega se lee con el adaptador
        // que le corresponde en vez de adivinarlo por el campo «object».
        .route(
            "/webhook/{canal}",
            get(webhook::verificar).post(webhook::recibir),
        )
        // Para el monitor del despliegue. No dice nada de la configuración: un
        // «vivo» que enumere lo que falta es un mapa para quien lo lea.
        .route("/salud", get(|| async { "vivo" }))
        .with_state(servicio)
}
