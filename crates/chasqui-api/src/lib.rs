// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! # chasqui-api
//!
//! El servidor: recibe los webhooks de Meta, decide con `chasqui-dominio` y
//! responde por el canal por el que entró. Y sirve el panel desde el que una
//! persona atiende lo que el bot escala.
//!
//! Es una librería además de un binario para que las pruebas puedan levantar el
//! router de verdad y hablarle por HTTP como le habla Meta (directiva 28).

#![forbid(unsafe_code)]
// `result_large_err`: FALSO POSITIVO en este crate (axum), como en medico-api. El
// error del handler es grande porque envuelve el error de quipu 0.15 —que creció
// respecto de 0.11—, no por un `Err` de dominio pesado; boxearlo en cada handler
// sería churn sin beneficio real. Se silencia a nivel de crate; quitar este allow
// vuelve a mostrar la lista completa.
#![allow(clippy::result_large_err)]

pub mod atender;
pub mod canales;
pub mod config;
pub mod consultas;
pub mod demo;
pub mod firma;
pub mod modelo;
pub mod panel;
pub mod servicio;
pub mod sesion;
pub mod vista;
pub mod web;
pub mod webhook;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, patch, post};

use servicio::Servicio;

// El panel ya no es un SPA de Angular embebido: se teje en el servidor con maud
// (módulo `vista`, directiva 26). No hay `rust-embed` ni `web/dist`; htmx y `boi`
// van vendorizados y servidos por `vista::{htmx_js, boi_js, ...}`.

/// La política de contenido de la APLICACIÓN. Ver [`cabeceras_de_seguridad`].
const CSP: &str = "default-src 'self'; \
                   script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data:; \
                   font-src 'self' data:; \
                   connect-src 'self'; \
                   object-src 'none'; \
                   base-uri 'self'; \
                   form-action 'self'; \
                   frame-ancestors 'none'";

/// Cabeceras que van en TODA respuesta.
///
/// **Van en la aplicación y no solo en el nginx de la guía**, y esa es la
/// decisión: Chasqui se instala en el servidor del cliente, y quien lo administre
/// puede poner Caddy, Apache o su propio proxy. Si las cabeceras vivieran
/// únicamente en nuestra plantilla, esa instalación se quedaría sin ninguna sin
/// que nada lo dijera. Es la lección que ya costó una pasada en `informes`.
///
/// # La CSP, probada contra el Angular compilado y no copiada de una lista
///
/// - **`script-src 'self'`, sin `'unsafe-inline'`.** Es la que para un XSS. Para
///   poder ponerla hubo que apagar `inlineCritical` en `angular.json`: emitía
///   `<link media="print" onload="this.media='all'">`, y ese `onload` es un
///   script en línea. Con esta directiva el navegador no lo ejecuta, la hoja se
///   queda en `media="print"` y **el panel se ve sin estilos sin dar un error**.
///   La prueba `el_index_empotrado_no_lleva_script_en_linea` falla si vuelve.
/// - **`style-src` lleva `'unsafe-inline'` y no es pereza.** Angular Material
///   escribe estilos en tiempo de ejecución y usa atributos `style="…"`; un
///   nonce vale para elementos `<style>`, no para atributos. Inyectar CSS es
///   mucho menos que ejecutar código.
/// - **`frame-ancestors 'none'`**: nadie enmarca el panel. El día que exista el
///   canal web, su widget SÍ tendrá que dejarse enmarcar por los dominios del
///   cliente — por eso la CSP no se pisa si la respuesta ya trae la suya: esa
///   ruta pondrá la que le toca, más permisiva a propósito y solo ahí.
/// - **Nada de HSTS aquí**: el TLS termina en el proxy del cliente, que es quien
///   puede prometerlo con conocimiento de causa. Va en la plantilla de nginx.
async fn cabeceras_de_seguridad(
    peticion: axum::extract::Request,
    siguiente: axum::middleware::Next,
) -> Response {
    let mut r = siguiente.run(peticion).await;
    let h = r.headers_mut();
    // No reinterpretes el tipo que declaro.
    h.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    // Sin esto, un sitio ajeno pone el panel en un iframe y engaña a quien
    // atiende para que pulse donde no cree.
    h.insert(header::X_FRAME_OPTIONS, header::HeaderValue::from_static("DENY"));
    // La ruta de una conversación no viaja como `Referer` a terceros.
    h.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    // El panel no usa ninguna de estas, y apagarlas cuesta una línea.
    h.insert(
        header::HeaderName::from_static("permissions-policy"),
        header::HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        h.insert(header::CONTENT_SECURITY_POLICY, header::HeaderValue::from_static(CSP));
    }
    r
}

/// El router completo.
pub fn router(servicio: Servicio) -> Router {
    let api = Router::new()
        // Entrar y salir. `entrar` NO pide sesión, por razones evidentes; es la
        // única de todo este bloque.
        .route("/sesion", post(panel::entrar).delete(panel::salir))
        .route("/yo", get(panel::yo))
        // SIN sesión a propósito: la lee la pantalla de entrada. Solo devuelve
        // lo que alguien escribió en el ajuste `demo.credenciales`, que en una
        // instalación normal está vacío. Ver `panel::demo`.
        .route("/demo", get(panel::demo))
        // La cola: lo que hace que «le paso con una persona» sea verdad.
        .route("/escalados", get(panel::escalados))
        .route("/escalados/{id}/atender", post(panel::atender_escalado))
        // Conversaciones e historial.
        .route("/conversaciones", get(panel::conversaciones))
        .route("/conversaciones/{id}", get(panel::historial))
        .route("/conversaciones/{id}/mensaje", post(panel::responder))
        .route("/conversaciones/{id}/devolver-al-bot", post(panel::devolver_al_bot))
        // El derecho de supresión, y la prueba de haberlo atendido.
        .route("/conversaciones/{id}/olvidar", post(panel::olvidar_contacto))
        // Los bytes de una foto o de una nota de voz. Con sesión, como todo
        // `/api`: es la foto que un cliente le mandó a un negocio.
        .route("/medios/{id}", get(panel::medio))
        .route("/supresiones", get(panel::supresiones))
        // Configuración.
        .route("/ajustes", get(panel::ajustes).put(panel::poner_ajuste))
        .route("/conocimiento", get(panel::preguntas).post(panel::crear_pregunta))
        .route("/conocimiento/{id}", patch(panel::activar_pregunta))
        .route("/consultas", get(panel::consultas))
        .route("/consultas/{id}", patch(panel::activar_consulta))
        // Los flujos de varios pasos y lo que producen.
        .route("/flujos", get(panel::flujos).post(panel::crear_flujo))
        .route("/flujos/{id}", patch(panel::activar_flujo).delete(panel::borrar_flujo))
        .route("/pedidos", get(panel::pedidos))
        .route("/cuentas", get(panel::cuentas))
        .route("/documentos", get(panel::documentos))
        .route("/documentos/{id}", patch(panel::activar_documento))
        .route("/modelo", get(panel::modelo));

    Router::new()
        // Las dos rutas que llama Meta, con el canal en la ruta. Un webhook por
        // canal y no uno solo: así el cuerpo que llega se lee con el adaptador
        // que le corresponde en vez de adivinarlo por el campo «object».
        .route("/webhook/{canal}", get(webhook::verificar).post(webhook::recibir))
        // El canal web: las tres rutas que hablan con el widget del sitio del
        // cliente y con su aplicación móvil. **Son las únicas sin sesión**, y por
        // eso van juntas y a la vista: comprueban el sitio contra las cuentas de
        // canal `web` dadas de alta, y llevan su propio freno por IP.
        //
        // No hay `/webhook/web`: ahí no hay nada que Meta firme.
        .route("/web/conversacion", post(web::abrir).options(web::previa))
        .route("/web/mensaje", post(web::mensaje).options(web::previa))
        .route("/web/mensajes", get(web::mensajes).options(web::previa))
        .route("/web/widget.js", get(web::widget))
        // El modo demostración: las dos mitades del producto en una pantalla.
        // TODAS estas rutas devuelven 404 salvo que el ajuste `demo.activa`
        // valga «si», que en la instalación de un cliente no ocurre nunca.
        .route("/demo", get(demo::pagina))
        .route("/demo.js", get(demo::guion))
        .route("/web/demo/panel", get(demo::panel).options(web::previa))
        .route("/web/demo/atender", post(demo::atender).options(web::previa))
        .route("/web/demo/responder", post(demo::responder).options(web::previa))
        .route("/web/demo/devolver", post(demo::devolver).options(web::previa))
        // Para el monitor del despliegue. No dice nada de la configuración: un
        // «vivo» que enumere lo que falta es un mapa para quien lo lea.
        .route("/salud", get(|| async { "vivo" }))
        .nest("/api", api)
        // El PANEL, tejido en el servidor con maud + htmx (módulo `vista`). Las
        // páginas exigen sesión por el extractor `vista::Panelista`, que sin
        // cookie válida redirige a `/entrar` (no un 401: lo abre una persona).
        .route("/", get(vista::raiz))
        .route("/entrar", get(vista::entrar_pagina).post(vista::entrar_enviar))
        .route("/salir", post(vista::salir_enviar))
        .route("/cola", get(vista::cola_pagina))
        .route("/cola/lista", get(vista::cola_lista))
        .route("/escalados/{id}/atender", post(vista::atender_enviar))
        .route("/conversacion/{id}", get(vista::conversacion_pagina))
        .route("/conversacion/{id}/hilo", get(vista::conversacion_hilo))
        .route("/conversacion/{id}/responder", post(vista::responder_enviar))
        .route("/conversacion/{id}/devolver", post(vista::devolver_enviar))
        .route("/conversacion/{id}/olvidar", post(vista::olvidar_enviar))
        .route("/pedidos", get(vista::pedidos_pagina))
        .route("/configurar", get(vista::configurar_pagina))
        .route("/configurar/ajuste", post(vista::guardar_ajuste))
        .route("/configurar/pregunta", post(vista::crear_pregunta))
        .route("/configurar/pregunta/{id}/activar", post(vista::activar_pregunta))
        .route("/configurar/consulta/{id}/activar", post(vista::activar_consulta))
        .route("/configurar/documento/{id}/activar", post(vista::activar_documento))
        .route("/configurar/flujo", post(vista::guardar_flujo))
        .route("/configurar/flujo/{id}/activar", post(vista::activar_flujo))
        .route("/configurar/flujo/{id}/borrar", post(vista::borrar_flujo))
        // Recursos estáticos: vendorizados y servidos LOCALES (nunca CDN).
        .route("/htmx.min.js", get(vista::htmx_js))
        .route("/boi.js", get(vista::boi_js))
        .route("/comportamientos.js", get(vista::comportamientos_js))
        .route("/comportamientos/confirmar-doble.js", get(vista::confirmar_doble_js))
        .route("/boi.css", get(vista::boi_css))
        .route("/panel.css", get(vista::css))
        .fallback(|| async { (StatusCode::NOT_FOUND, "no encontrado") })
        // Sobre TODO lo anterior, incluido el comodín y el webhook: una capa
        // puesta sobre un subconjunto de rutas es una que alguien olvidará al
        // añadir la siguiente.
        .layer(axum::middleware::from_fn(cabeceras_de_seguridad))
        .with_state(servicio)
}

// La invariante «la CSP estricta no rompe el panel» (ningún script en línea,
// ningún recurso externo) se prueba ahora sobre la salida de maud, en
// `vista::pruebas::la_envoltura_no_lleva_script_en_linea_ni_recurso_externo` —
// donde vive la función que teje ese HTML.
