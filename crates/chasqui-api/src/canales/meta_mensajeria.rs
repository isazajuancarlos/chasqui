// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Messenger e Instagram: la API de mensajería de Meta.
//!
//! Un solo adaptador para los dos porque comparten formato de webhook y de
//! envío; lo que cambia es el `object` de la raíz y el límite de caracteres. Esto
//! es lo que hace cierta la frase de la propuesta —«los tres canales por el
//! precio de uno y dos adaptadores»— y no una promesa.
//!
//! Dos trampas que separan esta API de la de WhatsApp, y que no se ven leyendo:
//!
//! 1. **El eco.** Meta reenvía por webhook lo que nosotros mismos enviamos, con
//!    `is_echo: true`. Atenderlo es un bucle infinito: el bot se responde a sí
//!    mismo hasta que Meta corta el número. Es el fallo más caro de este archivo.
//! 2. **El reloj va en milisegundos**, no en segundos como en WhatsApp. Leerlo
//!    como segundos pone los mensajes en el año 58 000.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use chasqui_dominio::mensaje::{Clase, Medio, Ubicacion};
use chasqui_dominio::{Canal, MensajeEntrante, MensajeSaliente};

use super::{AdaptadorCanal, Descarga, ErrorCanal};

/// Límites de texto de Meta. Instagram es bastante más corto que Messenger, y
/// pasarse no trunca: rechaza el envío entero.
const LIMITE_MESSENGER: usize = 2000;
const LIMITE_INSTAGRAM: usize = 1000;

pub struct MetaMensajeria {
    canal: Canal,
    cliente: reqwest::Client,
    graph: String,
}

impl MetaMensajeria {
    /// `canal` tiene que ser [`Canal::Messenger`] o [`Canal::Instagram`].
    /// WhatsApp tiene su propio adaptador: su formato no se parece.
    pub fn nuevo(canal: Canal, cliente: reqwest::Client, graph: impl Into<String>) -> Self {
        assert!(
            matches!(canal, Canal::Messenger | Canal::Instagram),
            "MetaMensajeria no atiende {canal}: WhatsApp usa la Cloud API, que es otro formato"
        );
        MetaMensajeria { canal, cliente, graph: graph.into() }
    }

    fn forma(&self, detalle: impl Into<String>) -> ErrorCanal {
        ErrorCanal::Forma { canal: self.canal, detalle: detalle.into() }
    }

    /// El momento del evento. Aquí Meta cuenta en MILISEGUNDOS.
    fn momento(&self, evento: &Value) -> Result<DateTime<Utc>, ErrorCanal> {
        let ms = evento
            .get("timestamp")
            .and_then(Value::as_i64)
            .ok_or_else(|| self.forma("un evento sin «timestamp»"))?;
        DateTime::from_timestamp_millis(ms)
            .ok_or_else(|| self.forma(format!("«timestamp» fuera de rango: {ms}")))
    }
}

/// Traduce un evento a su clase, o `None` si no es algo que haya que atender.
///
/// `None` es lo normal: acuses de entrega, confirmaciones de lectura y ecos
/// llegan por el mismo webhook y son la mayoría del tráfico.
fn clase(evento: &Value) -> Option<Clase> {
    // EL ECO PRIMERO. Es lo que enviamos nosotros, devuelto por Meta. Si se
    // atendiera, el bot se contestaría a sí mismo sin parar.
    if evento.pointer("/message/is_echo").and_then(Value::as_bool) == Some(true) {
        return None;
    }

    if let Some(postback) = evento.get("postback") {
        return Some(Clase::Interactivo {
            id: postback.get("payload").and_then(Value::as_str).unwrap_or_default().to_string(),
            titulo: postback.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
        });
    }

    let mensaje = evento.get("message")?;

    // Respuesta rápida: trae el payload que configuró el cliente.
    if let Some(rapida) = mensaje.pointer("/quick_reply/payload").and_then(Value::as_str) {
        return Some(Clase::Interactivo {
            id: rapida.to_string(),
            titulo: mensaje.get("text").and_then(Value::as_str).unwrap_or(rapida).to_string(),
        });
    }

    if let Some(texto) = mensaje.get("text").and_then(Value::as_str) {
        return Some(Clase::Texto(texto.to_string()));
    }

    // Adjuntos sin texto: una foto, un audio, un sticker, una ubicación.
    //
    // OJO — ESTO NO ESTÁ VERIFICADO CONTRA INSTAGRAM NI MESSENGER DE VERDAD.
    // WhatsApp sí lo está (2026-08-02, con un teléfono real); aquí no hay cuenta
    // con la que probar, así que la forma sale de la documentación de Meta — la
    // misma que ese día se quedó corta en seis campos. Por eso todo lo que no
    // case cae a `NoSoportado` con su nombre: si la forma es otra, el síntoma
    // será «no lo sé leer» y no un archivo inventado.
    if let Some(adjuntos) = mensaje.get("attachments").and_then(Value::as_array) {
        let primero = adjuntos.first();
        let tipo = primero
            .and_then(|a| a.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("desconocido");

        // La ubicación viaja como adjunto, pero con coordenadas en vez de URL.
        if tipo == "location" {
            let lat = primero.and_then(|a| a.pointer("/payload/coordinates/lat")).and_then(Value::as_f64);
            let lon = primero.and_then(|a| a.pointer("/payload/coordinates/long")).and_then(Value::as_f64);
            return Some(match (lat, lon) {
                (Some(latitud), Some(longitud)) => Clase::Ubicacion(Ubicacion {
                    latitud,
                    longitud,
                    nombre: primero
                        .and_then(|a| a.get("title"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    direccion: None,
                }),
                _ => Clase::NoSoportado("adjunto:location sin coordenadas".into()),
            });
        }

        // Aquí el billete NO es un identificador: es la URL misma. El campo
        // `id` de `Medio` es «con qué se pide el archivo», y en este canal eso
        // es una URL. Se distingue sola —empieza por http— y así el adaptador
        // no necesita una bandera más.
        let url = primero.and_then(|a| a.pointer("/payload/url")).and_then(Value::as_str);
        return Some(match url {
            Some(u) => Clase::Medio(Medio {
                tipo: tipo.to_string(),
                id: u.to_string(),
                mime: None,
                nombre_archivo: None,
            }),
            None => Clase::NoSoportado(format!("adjunto:{tipo}")),
        });
    }

    None
}

#[async_trait]
impl AdaptadorCanal for MetaMensajeria {
    fn canal(&self) -> Canal {
        self.canal
    }

    fn limite_texto(&self) -> usize {
        match self.canal {
            Canal::Instagram => LIMITE_INSTAGRAM,
            _ => LIMITE_MESSENGER,
        }
    }

    /// Aquí el «identificador» ya es la URL del archivo, así que es **una sola
    /// llamada** y no dos como en WhatsApp.
    ///
    /// Va SIN el token: son URLs firmadas de la CDN de Meta, y meterle una
    /// cabecera `Authorization` a una URL ya firmada es la clase de cosa que
    /// devuelve 403 sin explicar por qué. **Sin verificar contra una cuenta
    /// real** — como todo lo de medios en este canal.
    async fn descargar_medio(
        &self,
        _token: &str,
        url: &str,
        tope: usize,
    ) -> Result<Descarga, ErrorCanal> {
        if !url.starts_with("http") {
            return Err(self.forma(format!(
                "se esperaba la URL del adjunto y llegó {url:?}; en este canal el billete del \
                 medio es la URL, no un identificador"
            )));
        }

        let respuesta = self.cliente.get(url).send().await?.error_for_status()?;
        let mime = respuesta
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let bytes = respuesta.bytes().await?;
        if bytes.len() > tope {
            return Err(ErrorCanal::DemasiadoGrande { tamano: bytes.len() as u64, tope });
        }
        Ok(Descarga { bytes: bytes.to_vec(), mime })
    }

    fn entrantes(&self, cuerpo: &Value) -> Result<Vec<MensajeEntrante>, ErrorCanal> {
        let objeto = cuerpo
            .get("object")
            .and_then(Value::as_str)
            .ok_or_else(|| self.forma("el cuerpo no trae «object»"))?;
        if Some(objeto) != self.canal.objeto_meta() {
            return Err(self.forma(format!(
                "«object» es {objeto:?} y en la ruta de {} tiene que ser {:?}",
                self.canal,
                self.canal.objeto_meta()
            )));
        }

        let entradas = cuerpo
            .get("entry")
            .and_then(Value::as_array)
            .ok_or_else(|| self.forma("el cuerpo no trae «entry»"))?;

        let mut mensajes = Vec::new();
        for entrada in entradas {
            let eventos = entrada.get("messaging").and_then(Value::as_array);
            for evento in eventos.into_iter().flatten() {
                let Some(clase) = clase(evento) else { continue };

                let contacto = evento
                    .pointer("/sender/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.forma("un evento sin «sender.id»"))?;
                // La cuenta que recibió: la página o la cuenta profesional. Es
                // la que dice con qué token se responde.
                let cuenta = evento
                    .pointer("/recipient/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.forma("un evento sin «recipient.id»"))?;
                let id_externo = evento
                    .pointer("/message/mid")
                    .or_else(|| evento.pointer("/postback/mid"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| self.forma("un evento sin «mid»"))?;

                mensajes.push(MensajeEntrante {
                    canal: self.canal,
                    cuenta: cuenta.to_string(),
                    contacto: contacto.to_string(),
                    id_externo: id_externo.to_string(),
                    clase,
                    momento: self.momento(evento)?,
                });
            }
        }
        Ok(mensajes)
    }

    async fn enviar(
        &self,
        token: &str,
        cuenta: &str,
        mensaje: &MensajeSaliente,
    ) -> Result<Option<String>, ErrorCanal> {
        let url = format!("{}/{}/messages", self.graph, cuenta);
        let cuerpo = json!({
            "recipient": { "id": mensaje.para },
            // RESPONSE: es una respuesta a algo que escribió la persona. Meta
            // trata distinto lo que el negocio inicia, y decirlo mal es lo que
            // acaba en una restricción de la cuenta del cliente.
            "messaging_type": "RESPONSE",
            "message": { "text": mensaje.texto },
        });

        let respuesta = self.cliente.post(&url).bearer_auth(token).json(&cuerpo).send().await?;
        let estado = respuesta.status();
        let texto = respuesta.text().await.unwrap_or_default();

        if !estado.is_success() {
            return Err(ErrorCanal::Rechazado {
                canal: self.canal,
                estado: estado.as_u16(),
                cuerpo: texto,
            });
        }

        let acuse = serde_json::from_str::<Value>(&texto)
            .ok()
            .and_then(|v| v.get("message_id").and_then(Value::as_str).map(String::from));
        Ok(acuse)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn adaptador(canal: Canal) -> MetaMensajeria {
        MetaMensajeria::nuevo(canal, reqwest::Client::new(), "https://graph.example/v21.0")
    }

    fn webhook(objeto: &str, evento: Value) -> Value {
        json!({ "object": objeto, "entry": [{ "id": "PAGE-1", "time": 1785000000000i64,
                                              "messaging": [evento] }] })
    }

    fn mensaje_normal() -> Value {
        json!({
            "sender": { "id": "PSID-1" },
            "recipient": { "id": "PAGE-1" },
            "timestamp": 1785000000000i64,
            "message": { "mid": "m_abc", "text": "¿Cuál es el horario?" }
        })
    }

    #[test]
    fn traduce_un_mensaje_de_messenger() {
        let m = adaptador(Canal::Messenger)
            .entrantes(&webhook("page", mensaje_normal()))
            .unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].canal, Canal::Messenger);
        assert_eq!(m[0].cuenta, "PAGE-1");
        assert_eq!(m[0].contacto, "PSID-1");
        assert_eq!(m[0].id_externo, "m_abc");
        assert_eq!(m[0].clase, Clase::Texto("¿Cuál es el horario?".into()));
    }

    #[test]
    fn el_mismo_formato_sirve_para_instagram() {
        let m = adaptador(Canal::Instagram)
            .entrantes(&webhook("instagram", mensaje_normal()))
            .unwrap();
        assert_eq!(m[0].canal, Canal::Instagram);
        assert_eq!(m[0].clase, Clase::Texto("¿Cuál es el horario?".into()));
    }

    #[test]
    fn el_eco_de_lo_que_enviamos_no_se_atiende() {
        // LA prueba de este archivo. Sin ella el bot se responde a sí mismo en
        // bucle, y quien paga la fiesta es el número del cliente.
        let eco = json!({
            "sender": { "id": "PAGE-1" },
            "recipient": { "id": "PSID-1" },
            "timestamp": 1785000000000i64,
            "message": { "mid": "m_eco", "text": "De 8 a 6.", "is_echo": true }
        });
        assert!(adaptador(Canal::Messenger).entrantes(&webhook("page", eco)).unwrap().is_empty());
    }

    #[test]
    fn los_acuses_de_entrega_y_lectura_se_ignoran() {
        for evento in [
            json!({ "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
                    "timestamp": 1785000000000i64, "delivery": { "watermark": 1785000000000i64 } }),
            json!({ "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
                    "timestamp": 1785000000000i64, "read": { "watermark": 1785000000000i64 } }),
        ] {
            assert!(
                adaptador(Canal::Messenger).entrantes(&webhook("page", evento)).unwrap().is_empty()
            );
        }
    }

    #[test]
    fn el_reloj_se_lee_en_milisegundos() {
        // El caso rojo: leerlo como segundos pondría este mensaje en el año
        // 58 000, y el historial del cliente saldría ordenado al revés.
        let m = adaptador(Canal::Messenger)
            .entrantes(&webhook("page", mensaje_normal()))
            .unwrap();
        assert_eq!(m[0].momento.timestamp(), 1785000000);
    }

    #[test]
    fn un_postback_de_boton_llega_como_interactivo() {
        let evento = json!({
            "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
            "timestamp": 1785000000000i64,
            "postback": { "mid": "m_pb", "title": "Ver precios", "payload": "ver_precios" }
        });
        let m = adaptador(Canal::Messenger).entrantes(&webhook("page", evento)).unwrap();
        assert_eq!(
            m[0].clase,
            Clase::Interactivo { id: "ver_precios".into(), titulo: "Ver precios".into() }
        );
    }

    #[test]
    fn una_foto_sin_texto_conserva_su_url_para_poder_traerla() {
        // Aquí el billete es la URL y no un identificador: es la diferencia con
        // WhatsApp, y por eso `descargar_medio` de este canal hace UNA llamada.
        let evento = json!({
            "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
            "timestamp": 1785000000000i64,
            "message": { "mid": "m_img",
                         "attachments": [{ "type": "image", "payload": { "url": "https://cdn.example/f.jpg" } }] }
        });
        let m = adaptador(Canal::Messenger).entrantes(&webhook("page", evento)).unwrap();
        let Clase::Medio(medio) = &m[0].clase else { panic!("{:?}", m[0].clase) };
        assert_eq!(medio.tipo, "image");
        assert_eq!(medio.id, "https://cdn.example/f.jpg");
    }

    #[test]
    fn un_adjunto_sin_url_se_denuncia_en_vez_de_prometer_un_archivo() {
        // ESTA FORMA NO ESTÁ VERIFICADA CONTRA INSTAGRAM REAL. Si Meta la manda
        // de otra manera, el síntoma tiene que ser «no lo sé leer» y no una
        // ficha de archivo que nunca se podrá descargar.
        let evento = json!({
            "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
            "timestamp": 1785000000000i64,
            "message": { "mid": "m_img", "attachments": [{ "type": "image", "payload": {} }] }
        });
        let m = adaptador(Canal::Messenger).entrantes(&webhook("page", evento)).unwrap();
        assert_eq!(m[0].clase, Clase::NoSoportado("adjunto:image".into()));
    }

    #[test]
    fn una_ubicacion_de_messenger_trae_sus_coordenadas() {
        let evento = json!({
            "sender": { "id": "PSID-1" }, "recipient": { "id": "PAGE-1" },
            "timestamp": 1785000000000i64,
            "message": { "mid": "m_loc", "attachments": [{
                "type": "location", "title": "Mi casa",
                "payload": { "coordinates": { "lat": 6.038, "long": -75.702 } }
            }] }
        });
        let m = adaptador(Canal::Messenger).entrantes(&webhook("page", evento)).unwrap();
        let Clase::Ubicacion(u) = &m[0].clase else { panic!("{:?}", m[0].clase) };
        assert_eq!(u.latitud, 6.038);
        assert_eq!(u.longitud, -75.702);
        assert_eq!(u.nombre.as_deref(), Some("Mi casa"));
    }

    #[test]
    fn un_webhook_de_instagram_en_la_ruta_de_messenger_se_denuncia() {
        let r = adaptador(Canal::Messenger).entrantes(&webhook("instagram", mensaje_normal()));
        assert!(matches!(r, Err(ErrorCanal::Forma { .. })));
    }

    #[test]
    fn instagram_admite_menos_texto_que_messenger() {
        assert!(adaptador(Canal::Instagram).limite_texto() < adaptador(Canal::Messenger).limite_texto());
    }

    #[test]
    #[should_panic(expected = "no atiende whatsapp")]
    fn no_se_deja_construir_para_whatsapp() {
        // WhatsApp tiene otro formato de webhook y de envío. Construir este
        // adaptador para WhatsApp compilaría y fallaría en producción.
        adaptador(Canal::WhatsApp);
    }
}
