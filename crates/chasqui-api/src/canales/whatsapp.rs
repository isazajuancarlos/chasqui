// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! WhatsApp Cloud API: el canal que piden las cinco peticiones.
//!
//! **La oficial, y ninguna otra.** Baileys y `whatsapp-web.js` funcionan hasta
//! que Meta cierra el número del cliente, y el día que a un negocio que vive del
//! WhatsApp le bloquean el número responde quien se lo montó. Está rechazado por
//! escrito en la propuesta; aquí se rechaza por construcción, que es lo que de
//! verdad lo impide.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use chasqui_dominio::mensaje::{Clase, Medio, Ubicacion};
use chasqui_dominio::{Canal, MensajeEntrante, MensajeSaliente};

use super::{AdaptadorCanal, Descarga, ErrorCanal};

/// Máximo de un mensaje de texto en WhatsApp. Lo fija Meta.
const LIMITE_TEXTO: usize = 4096;

pub struct WhatsApp {
    cliente: reqwest::Client,
    graph: String,
}

impl WhatsApp {
    pub fn nuevo(cliente: reqwest::Client, graph: impl Into<String>) -> Self {
        WhatsApp { cliente, graph: graph.into() }
    }
}

fn forma(detalle: impl Into<String>) -> ErrorCanal {
    ErrorCanal::Forma { canal: Canal::WhatsApp, detalle: detalle.into() }
}

/// El momento que dice Meta, en segundos desde la época y como cadena.
///
/// Si no viene o no se puede leer, es un fallo de forma y no un «pues ahora
/// mismo». Poner la hora de recepción sería inventar un dato que después alguien
/// lee como hora de envío (directiva 20).
fn momento(m: &Value) -> Result<DateTime<Utc>, ErrorCanal> {
    let texto = m
        .get("timestamp")
        .and_then(Value::as_str)
        .ok_or_else(|| forma("un mensaje sin «timestamp»"))?;
    let segundos: i64 = texto
        .parse()
        .map_err(|_| forma(format!("«timestamp» no es un número de segundos: {texto:?}")))?;
    DateTime::from_timestamp(segundos, 0)
        .ok_or_else(|| forma(format!("«timestamp» fuera de rango: {segundos}")))
}

/// Una cadena de dentro del mensaje, o `None` si no está o está vacía.
///
/// Vacía cuenta como ausente **a propósito**: un `filename` de "" guardado como
/// `Some("")` produce después un archivo sin nombre que nadie sabe de dónde
/// salió. Si no hay dato, que se diga que no hay (directiva 20).
fn cadena(m: &Value, puntero: &str) -> Option<String> {
    m.pointer(puntero)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Traduce un mensaje de WhatsApp a su clase.
///
/// Lo que no se sabe leer se marca `NoSoportado` con el nombre que le da Meta, y
/// más arriba eso llama a una persona. Un audio o una foto NO se convierten en
/// texto vacío: el cliente nunca sabría que le mandaron una nota de voz.
fn clase(m: &Value) -> Clase {
    let tipo = m.get("type").and_then(Value::as_str).unwrap_or("desconocido");

    match tipo {
        "text" => match m.pointer("/text/body").and_then(Value::as_str) {
            Some(t) => Clase::Texto(t.to_string()),
            // Un `text` sin cuerpo no es un texto vacío: es un mensaje que no
            // entendimos, y va a una persona.
            None => Clase::NoSoportado("text sin body".into()),
        },
        // Botones y listas del mensaje interactivo.
        "interactive" => {
            let respuesta = m
                .pointer("/interactive/button_reply")
                .or_else(|| m.pointer("/interactive/list_reply"));
            match respuesta {
                Some(r) => Clase::Interactivo {
                    id: r.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                    titulo: r.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                },
                None => Clase::NoSoportado("interactive desconocido".into()),
            }
        }
        // La ubicación NO se descarga: las coordenadas vienen en el webhook.
        //
        // Si faltan, esto NO inventa un 0,0 —que es una isla del golfo de
        // Guinea y una dirección de reparto perfectamente creíble para un
        // programa—: se cae a «no soportado» y va a una persona (directiva 20).
        // Es también la sonda que avisa si Meta cambia el nombre del campo.
        "location" => {
            let lat = m.pointer("/location/latitude").and_then(Value::as_f64);
            let lon = m.pointer("/location/longitude").and_then(Value::as_f64);
            match (lat, lon) {
                (Some(latitud), Some(longitud)) => Clase::Ubicacion(Ubicacion {
                    latitud,
                    longitud,
                    nombre: cadena(m, "/location/name"),
                    direccion: cadena(m, "/location/address"),
                }),
                _ => Clase::NoSoportado("location sin coordenadas".into()),
            }
        }
        // Los medios traen un identificador, no los bytes. Sin identificador el
        // archivo es irrecuperable, así que un medio sin `id` tampoco se
        // maquilla: se nombra y va a una persona.
        "image" | "audio" | "video" | "document" | "sticker" | "voice" => {
            match cadena(m, &format!("/{tipo}/id")) {
                Some(id) => Clase::Medio(Medio {
                    tipo: tipo.to_string(),
                    id,
                    mime: cadena(m, &format!("/{tipo}/mime_type")),
                    nombre_archivo: cadena(m, &format!("/{tipo}/filename")),
                }),
                None => Clase::NoSoportado(format!("{tipo} sin id")),
            }
        }
        // Respuesta rápida de una plantilla: viene con otra forma.
        "button" => Clase::Interactivo {
            id: m.pointer("/button/payload").and_then(Value::as_str).unwrap_or_default().to_string(),
            titulo: m.pointer("/button/text").and_then(Value::as_str).unwrap_or_default().to_string(),
        },
        otro => Clase::NoSoportado(otro.to_string()),
    }
}

#[async_trait]
impl AdaptadorCanal for WhatsApp {
    fn canal(&self) -> Canal {
        Canal::WhatsApp
    }

    fn limite_texto(&self) -> usize {
        LIMITE_TEXTO
    }

    /// Baja un medio. **Dos llamadas, y la segunda no va a Graph.**
    ///
    /// La primera cambia el identificador por una URL de un servidor de
    /// descargas de Meta, que **caduca en minutos**: por eso esto se llama al
    /// recibir el mensaje y no cuando el agente abre el panel, donde la URL ya
    /// estaría muerta.
    ///
    /// La segunda llamada **también lleva el token**, aunque la URL parezca
    /// completa: sin la cabecera devuelve 401. Es de las cosas que solo se
    /// aprenden probando.
    async fn descargar_medio(
        &self,
        token: &str,
        id: &str,
        tope: usize,
    ) -> Result<Descarga, ErrorCanal> {
        let ficha: Value = self
            .cliente
            .get(format!("{}/{id}", self.graph))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let url = ficha
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| forma(format!("la ficha del medio {id} no trae «url»")))?;

        // El tamaño que declara Meta se mira ANTES de bajar nada: negarse
        // después de haberse tragado 90 MB no evita el gasto, solo el guardado.
        if let Some(tamano) = ficha.get("file_size").and_then(Value::as_u64)
            && tamano > tope as u64
        {
            return Err(ErrorCanal::DemasiadoGrande { tamano, tope });
        }

        // La URL la eligió la respuesta de Graph, no nosotros: se comprueba
        // igual que la que llega por webhook en Instagram y Messenger. Es el
        // token del cliente lo que se le adjunta a esta petición.
        if let Err(porque) = super::url_de_descarga_admisible(url, &self.graph) {
            return Err(forma(format!("la URL del medio {id} no se puede pedir: {porque}")));
        }

        // Y otra vez el tope, porque `file_size` puede no venir: tiene que valer
        // también cuando Meta no dice cuánto pesa. Se corta MIENTRAS baja.
        super::bajar_con_tope(self.cliente.get(url).bearer_auth(token), tope).await
    }

    fn entrantes(&self, cuerpo: &Value) -> Result<Vec<MensajeEntrante>, ErrorCanal> {
        let objeto = cuerpo
            .get("object")
            .and_then(Value::as_str)
            .ok_or_else(|| forma("el cuerpo no trae «object»"))?;
        if Some(objeto) != Canal::WhatsApp.objeto_meta() {
            return Err(forma(format!(
                "«object» es {objeto:?} y en la ruta de WhatsApp tiene que ser {:?}",
                Canal::WhatsApp.objeto_meta()
            )));
        }

        let entradas = cuerpo
            .get("entry")
            .and_then(Value::as_array)
            .ok_or_else(|| forma("el cuerpo no trae «entry»"))?;

        let mut mensajes = Vec::new();
        for entrada in entradas {
            let cambios = entrada.get("changes").and_then(Value::as_array);
            for cambio in cambios.into_iter().flatten() {
                let valor = match cambio.get("value") {
                    Some(v) => v,
                    None => continue,
                };

                // Los acuses de entrega y lectura llegan por aquí con
                // «statuses» en vez de «messages». No son un error: son la
                // mayoría del tráfico.
                let Some(lista) = valor.get("messages").and_then(Value::as_array) else {
                    continue;
                };

                // Sin esto no se sabe con qué número —ni con qué token—
                // responder, así que no hay forma de atender el mensaje.
                let cuenta = valor
                    .pointer("/metadata/phone_number_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| forma("falta metadata.phone_number_id"))?;

                for m in lista {
                    let id_externo = m
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| forma("un mensaje sin «id»"))?;
                    let contacto = m
                        .get("from")
                        .and_then(Value::as_str)
                        .ok_or_else(|| forma("un mensaje sin «from»"))?;

                    mensajes.push(MensajeEntrante {
                        canal: Canal::WhatsApp,
                        cuenta: cuenta.to_string(),
                        contacto: contacto.to_string(),
                        id_externo: id_externo.to_string(),
                        clase: clase(m),
                        momento: momento(m)?,
                    });
                }
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
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": mensaje.para,
            "type": "text",
            "text": { "preview_url": false, "body": mensaje.texto },
        });

        let respuesta = self
            .cliente
            .post(&url)
            .bearer_auth(token)
            .json(&cuerpo)
            .send()
            .await?;

        let estado = respuesta.status();
        let texto = respuesta.text().await.unwrap_or_default();

        if !estado.is_success() {
            return Err(ErrorCanal::Rechazado {
                canal: Canal::WhatsApp,
                estado: estado.as_u16(),
                cuerpo: texto,
            });
        }

        // El acuse es útil pero no imprescindible: el mensaje ya salió. Si Meta
        // cambia la forma de la respuesta, no se pierde el envío por eso.
        let acuse = serde_json::from_str::<Value>(&texto)
            .ok()
            .and_then(|v| v.pointer("/messages/0/id").and_then(Value::as_str).map(String::from));
        Ok(acuse)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn adaptador() -> WhatsApp {
        WhatsApp::nuevo(reqwest::Client::new(), "https://graph.example/v21.0")
    }

    /// El cuerpo tal como lo manda Meta. Copiado de la forma documentada, no
    /// inventado: una prueba contra un JSON que nos hemos imaginado mide el
    /// códec, no el producto.
    fn webhook_de_texto() -> Value {
        json!({
          "object": "whatsapp_business_account",
          "entry": [{
            "id": "WABA-1",
            "changes": [{
              "field": "messages",
              "value": {
                "messaging_product": "whatsapp",
                "metadata": { "display_phone_number": "573001112233", "phone_number_id": "PNID-1" },
                "contacts": [{ "profile": { "name": "Ana" }, "wa_id": "573009998877" }],
                "messages": [{
                  "from": "573009998877",
                  "id": "wamid.HBgM",
                  "timestamp": "1785000000",
                  "text": { "body": "¿Cuál es el horario?" },
                  "type": "text"
                }]
              }
            }]
          }]
        })
    }

    #[test]
    fn traduce_un_mensaje_de_texto() {
        let m = adaptador().entrantes(&webhook_de_texto()).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].canal, Canal::WhatsApp);
        assert_eq!(m[0].cuenta, "PNID-1");
        assert_eq!(m[0].contacto, "573009998877");
        assert_eq!(m[0].id_externo, "wamid.HBgM");
        assert_eq!(m[0].clase, Clase::Texto("¿Cuál es el horario?".into()));
        assert_eq!(m[0].momento.timestamp(), 1785000000);
    }

    /// El cuerpo **REAL** que mandó Meta el 2026-08-02, capturado en la consola
    /// con un WhatsApp de verdad escribiéndole al número de prueba.
    ///
    /// La forma es literal; los valores que identifican a una persona están
    /// cambiados —nombre, teléfono e identificadores—, porque lo que esta prueba
    /// tiene que fijar es la FORMA, y un fixture no es sitio para el número de
    /// nadie.
    ///
    /// Trae **seis campos que la documentación no menciona**: `profile.username`,
    /// `contacts[].user_id`, `contacts[].country_code`, `messages[].from_user_id`,
    /// `messages[].from_logical_id` e `internal_1p_only_data`. Ninguno rompe el
    /// traductor porque se recorre `Value` campo a campo en vez de deserializar a
    /// una estructura cerrada — y eso, que parecía una decisión de comodidad,
    /// resulta ser la que sobrevive a que Meta añada cosas sin avisar.
    fn webhook_real_de_meta() -> Value {
        json!({
          "object": "whatsapp_business_account",
          "entry": [{
            "id": "1903916413906447",
            "changes": [{
              "field": "messages",
              "value": {
                "messaging_product": "whatsapp",
                "metadata": {
                  "display_phone_number": "15556705915",
                  "phone_number_id": "1221911424344798"
                },
                "contacts": [{
                  "profile": { "name": "Ana Prueba", "username": "anaprueba" },
                  "wa_id": "573001112233",
                  "user_id": "CO.0000000000000000",
                  "country_code": "CO"
                }],
                "messages": [{
                  "from": "573001112233",
                  "from_user_id": "CO.0000000000000000",
                  "id": "wamid.HBgMNTczMDAxMTEyMjMzFQIAEhggQUMzM0ZGNzA1RENEQzJGOEY1NEM0NjI5Q0MxODkyMUUA",
                  "timestamp": "1785685856",
                  "text": { "body": "Respuesta de prueba" },
                  "from_logical_id": "63363669340208",
                  "type": "text",
                  "internal_1p_only_data": {
                    "account_context": {
                      "waac_id": "1762499561569942",
                      "cs_id": "1221911424344798",
                      "account_context_type": "non_paid_messaging"
                    }
                  }
                }]
              }
            }]
          }]
        })
    }

    #[test]
    fn traduce_el_cuerpo_real_de_meta_con_sus_campos_no_documentados() {
        let m = adaptador().entrantes(&webhook_real_de_meta()).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].cuenta, "1221911424344798");
        assert_eq!(m[0].contacto, "573001112233");
        assert_eq!(m[0].clase, Clase::Texto("Respuesta de prueba".into()));
        assert_eq!(m[0].momento.timestamp(), 1785685856);
        // `type` va DESPUÉS de `text` en lo que manda Meta, y antes en la
        // documentación. Da igual —es JSON—, pero es justo la clase de detalle
        // por la que se captura el cuerpo real en vez de copiar el del manual.
    }

    #[test]
    fn los_acuses_de_entrega_no_son_mensajes_ni_son_un_error() {
        // Son la mayoría del tráfico. Tratarlos como fallo llenaría el log hasta
        // que nadie lo mire — y entonces el fallo de verdad tampoco se ve.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "id": "WABA-1", "changes": [{ "field": "messages", "value": {
            "messaging_product": "whatsapp",
            "metadata": { "phone_number_id": "PNID-1" },
            "statuses": [{ "id": "wamid.X", "status": "delivered", "timestamp": "1785000000" }]
          }}]}]
        });
        assert!(adaptador().entrantes(&cuerpo).unwrap().is_empty());
    }

    #[test]
    fn un_boton_llega_como_interactivo() {
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573009998877", "id": "wamid.B", "timestamp": "1785000000",
              "type": "interactive",
              "interactive": { "type": "button_reply",
                               "button_reply": { "id": "ver_precios", "title": "Ver precios" } }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        assert_eq!(
            m[0].clase,
            Clase::Interactivo { id: "ver_precios".into(), titulo: "Ver precios".into() }
        );
    }

    #[test]
    fn una_nota_de_voz_conserva_su_billete_en_vez_de_perderse() {
        // Esta prueba fijaba lo contrario hasta el 2026-08-02: comprobaba que un
        // audio se marcaba `NoSoportado("audio")`. Aquello parecía prudente y era
        // una fuga — el identificador se tiraba, y con la Cloud API no hay
        // bandeja de WhatsApp donde el negocio pueda ir a buscar el archivo. Lo
        // que se conserva del comportamiento viejo es lo importante: sigue sin
        // haber texto que buscar, así que sigue yendo a una persona.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573009998877", "id": "wamid.A", "timestamp": "1785000000",
              "type": "audio", "audio": { "id": "media-1", "mime_type": "audio/ogg" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        let Clase::Medio(medio) = &m[0].clase else { panic!("{:?}", m[0].clase) };
        assert_eq!(medio.id, "media-1");
        assert_eq!(m[0].clase.texto(), None, "un audio no se busca en la base de conocimiento");
    }

    #[test]
    fn un_webhook_de_otro_canal_en_esta_ruta_se_denuncia() {
        let cuerpo = json!({ "object": "page", "entry": [] });
        assert!(matches!(adaptador().entrantes(&cuerpo), Err(ErrorCanal::Forma { .. })));
    }

    #[test]
    fn sin_phone_number_id_no_se_inventa_una_cuenta() {
        // El caso rojo de la directiva 20: si esto cogiera «la primera cuenta
        // dada de alta», el mensaje se contestaría desde OTRO número del cliente.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "display_phone_number": "573001112233" },
            "messages": [{ "from": "573009998877", "id": "wamid.A", "timestamp": "1785000000",
                           "type": "text", "text": { "body": "hola" } }]
          }}]}]
        });
        let r = adaptador().entrantes(&cuerpo);
        assert!(matches!(&r, Err(ErrorCanal::Forma { detalle, .. }) if detalle.contains("phone_number_id")));
    }

    #[test]
    fn sin_timestamp_se_denuncia_en_vez_de_poner_la_hora_actual() {
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{ "from": "573009998877", "id": "wamid.A",
                           "type": "text", "text": { "body": "hola" } }]
          }}]}]
        });
        assert!(matches!(adaptador().entrantes(&cuerpo), Err(ErrorCanal::Forma { .. })));
    }

    #[test]
    fn un_lote_con_varios_mensajes_sale_entero() {
        // Meta agrupa. Si el traductor se quedara con el primero, el segundo
        // cliente no recibiría respuesta y no habría ningún error.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [
              { "from": "573001", "id": "wamid.1", "timestamp": "1785000000",
                "type": "text", "text": { "body": "uno" } },
              { "from": "573002", "id": "wamid.2", "timestamp": "1785000001",
                "type": "text", "text": { "body": "dos" } }
            ]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].contacto, "573002");
    }

    #[test]
    fn un_cuerpo_vacio_no_es_un_panico() {
        let cuerpo = json!({ "object": "whatsapp_business_account", "entry": [] });
        assert!(adaptador().entrantes(&cuerpo).unwrap().is_empty());
    }

    #[test]
    fn una_ubicacion_trae_sus_coordenadas_y_no_se_pierde() {
        // Las coordenadas vienen en el propio webhook: no hay nada que
        // descargar, y hasta el 2026-08-02 se tiraban.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.U", "timestamp": "1785000000",
              "type": "location",
              "location": { "latitude": 6.038, "longitude": -75.702, "name": "Casa", "address": "Cra 50 #10-20" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        let Clase::Ubicacion(u) = &m[0].clase else { panic!("no la leyó: {:?}", m[0].clase) };
        assert_eq!(u.latitud, 6.038);
        assert_eq!(u.longitud, -75.702);
        assert_eq!(u.nombre.as_deref(), Some("Casa"));
        assert_eq!(u.direccion.as_deref(), Some("Cra 50 #10-20"));
    }

    #[test]
    fn una_ubicacion_sin_coordenadas_no_se_inventa_un_cero_cero() {
        // 0,0 es una isla del golfo de Guinea, y para un programa es una
        // dirección de reparto perfectamente creíble. Directiva 20: se dice que
        // no se sabe.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.U2", "timestamp": "1785000000",
              "type": "location", "location": { "name": "Casa" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        assert_eq!(m[0].clase, Clase::NoSoportado("location sin coordenadas".into()));
    }

    #[test]
    fn una_foto_conserva_el_identificador_con_el_que_se_recupera() {
        // Sin el `id` el archivo es irrecuperable para siempre: era la fuga.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.F", "timestamp": "1785000000",
              "type": "image",
              "image": { "id": "MEDIA-123", "mime_type": "image/jpeg", "sha256": "abc" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        let Clase::Medio(medio) = &m[0].clase else { panic!("no la leyó: {:?}", m[0].clase) };
        assert_eq!(medio.tipo, "image");
        assert_eq!(medio.id, "MEDIA-123");
        assert_eq!(medio.mime.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn un_documento_conserva_el_nombre_que_le_puso_quien_lo_manda() {
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.D", "timestamp": "1785000000",
              "type": "document",
              "document": { "id": "MEDIA-9", "mime_type": "application/pdf", "filename": "formula.pdf" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        let Clase::Medio(medio) = &m[0].clase else { panic!("{:?}", m[0].clase) };
        assert_eq!(medio.nombre_archivo.as_deref(), Some("formula.pdf"));
    }

    #[test]
    fn un_medio_sin_id_se_denuncia_en_vez_de_fingir_que_llegó() {
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.X", "timestamp": "1785000000",
              "type": "audio", "audio": { "mime_type": "audio/ogg" }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        assert_eq!(m[0].clase, Clase::NoSoportado("audio sin id".into()));
    }

    #[test]
    fn el_nombre_de_archivo_vacio_cuenta_como_ausente() {
        // Un `Some("")` produce después un archivo sin nombre que nadie sabe de
        // dónde salió.
        let cuerpo = json!({
          "object": "whatsapp_business_account",
          "entry": [{ "changes": [{ "value": {
            "metadata": { "phone_number_id": "PNID-1" },
            "messages": [{
              "from": "573001", "id": "wamid.V", "timestamp": "1785000000",
              "type": "document", "document": { "id": "M-1", "filename": "   " }
            }]
          }}]}]
        });
        let m = adaptador().entrantes(&cuerpo).unwrap();
        let Clase::Medio(medio) = &m[0].clase else { panic!("{:?}", m[0].clase) };
        assert_eq!(medio.nombre_archivo, None);
    }
}
