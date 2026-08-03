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
pub mod demo;
pub mod firma;
pub mod modelo;
pub mod panel;
pub mod servicio;
pub mod sesion;
pub mod web;
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

/// La política de contenido de la APLICACIÓN. Ver [`cabeceras_de_seguridad`].
const CSP: &str = "default-src 'self'; \
                   script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data:; \
                   font-src 'self'; \
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
        // Lo demás es la interfaz. Va la ÚLTIMA: así una ruta de la API nunca
        // queda tapada por el comodín.
        .fallback(estatico)
        // Sobre TODO lo anterior, incluido el comodín y el webhook: una capa
        // puesta sobre un subconjunto de rutas es una que alguien olvidará al
        // añadir la siguiente.
        .layer(axum::middleware::from_fn(cabeceras_de_seguridad))
        .with_state(servicio)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_index_empotrado_no_lleva_script_en_linea() {
        // La salvaguarda del apagado de `inlineCritical`. Si vuelve a colarse un
        // manejador en línea, la CSP lo bloquea y el panel se ve sin estilos SIN
        // dar ningún error — el fallo que aparece el día de la demostración.
        // Se lee el archivo EMPOTRADO y no el del disco: es lo que se sirve.
        let indice = Interfaz::get("index.html").expect("la interfaz tiene que estar construida");
        let html = String::from_utf8_lossy(&indice.data);

        assert!(
            !html.contains("onload="),
            "el index empotrado trae un manejador en línea; la CSP lo bloqueará y el panel \
             saldrá sin estilos. Compruebe `inlineCritical` en interfaz/angular.json"
        );
        assert!(
            !html.contains("<style"),
            "el index empotrado trae un <style> en línea; con style-src estricto dejaría de \
             aplicarse"
        );
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "el index empotrado sale a un recurso externo: la CSP lo bloquea, y además rompe \
             la promesa de que la instalación funciona sin salir a internet"
        );
    }
}
