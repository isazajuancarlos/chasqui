// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! # chasqui-api
//!
//! El servidor: recibe los webhooks de Meta, decide con `chasqui-dominio` y
//! responde por el canal por el que entró. Y sirve el panel desde el que una
//! persona atiende lo que el bot escala.
//!
//! Es una librería además de un binario para que las pruebas puedan levantar el
//! router de verdad y hablarle por HTTP como le habla Meta (directiva 28).

#![forbid(unsafe_code)]

pub mod atender;
pub mod canales;
pub mod config;
pub mod consultas;
pub mod firma;
pub mod panel;
pub mod servicio;
pub mod sesion;
pub mod webhook;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};

use servicio::Servicio;

/// La interfaz, empotrada en el binario.
///
/// Un solo ejecutable en el servidor del cliente: sin carpetas que copiar al
/// desplegar ni un servidor de estáticos que configurar aparte. `build.rs` se
/// encarga de que la carpeta exista aunque no se haya construido la interfaz.
#[derive(rust_embed::Embed)]
#[folder = "../../interfaz/dist/interfaz/browser"]
struct Interfaz;

/// Sirve un archivo de la interfaz, o `index.html` si la ruta no es un archivo.
///
/// Esa caída a `index.html` es lo que hace que recargar el navegador en
/// `/conversaciones/7` funcione: las rutas las resuelve Angular en el navegador,
/// y el servidor no las conoce. Sin ella, cualquier recarga da 404 — el fallo
/// que aparece el día de la demostración y nunca al desarrollar.
async fn estatico(uri: axum::http::Uri) -> Response {
    let ruta = uri.path().trim_start_matches('/');
    let ruta = if ruta.is_empty() { "index.html" } else { ruta };

    if let Some(archivo) = Interfaz::get(ruta) {
        let tipo = archivo.metadata.mimetype();
        return ([(header::CONTENT_TYPE, tipo)], archivo.data).into_response();
    }

    match Interfaz::get("index.html") {
        Some(indice) => {
            ([(header::CONTENT_TYPE, "text/html")], indice.data).into_response()
        }
        // Solo si el binario se construyó sin interfaz Y sin el marcador de
        // `build.rs`. Se dice qué pasa en vez de devolver una página en blanco.
        None => (
            StatusCode::NOT_FOUND,
            "Este binario no lleva la interfaz dentro. Construya con: \
             cd interfaz && npm run build",
        )
            .into_response(),
    }
}

/// El router completo.
pub fn router(servicio: Servicio) -> Router {
    let api = Router::new()
        // Entrar y salir. `entrar` NO pide sesión, por razones evidentes; es la
        // única de todo este bloque.
        .route("/sesion", post(panel::entrar).delete(panel::salir))
        .route("/yo", get(panel::yo))
        // La cola: lo que hace que «le paso con una persona» sea verdad.
        .route("/escalados", get(panel::escalados))
        .route("/escalados/{id}/atender", post(panel::atender_escalado))
        // Conversaciones e historial.
        .route("/conversaciones", get(panel::conversaciones))
        .route("/conversaciones/{id}", get(panel::historial))
        .route("/conversaciones/{id}/mensaje", post(panel::responder))
        .route("/conversaciones/{id}/devolver-al-bot", post(panel::devolver_al_bot))
        // Configuración.
        .route("/ajustes", get(panel::ajustes).put(panel::poner_ajuste))
        .route("/conocimiento", get(panel::preguntas).post(panel::crear_pregunta))
        .route("/conocimiento/{id}", patch(panel::activar_pregunta))
        .route("/consultas", get(panel::consultas))
        .route("/consultas/{id}", patch(panel::activar_consulta))
        .route("/cuentas", get(panel::cuentas));

    Router::new()
        // Las dos rutas que llama Meta, con el canal en la ruta. Un webhook por
        // canal y no uno solo: así el cuerpo que llega se lee con el adaptador
        // que le corresponde en vez de adivinarlo por el campo «object».
        .route("/webhook/{canal}", get(webhook::verificar).post(webhook::recibir))
        // Para el monitor del despliegue. No dice nada de la configuración: un
        // «vivo» que enumere lo que falta es un mapa para quien lo lea.
        .route("/salud", get(|| async { "vivo" }))
        .nest("/api", api)
        // Lo demás es la interfaz. Va la ÚLTIMA: así una ruta de la API nunca
        // queda tapada por el comodín.
        .fallback(estatico)
        .with_state(servicio)
}
