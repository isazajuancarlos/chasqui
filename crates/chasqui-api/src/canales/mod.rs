// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Los adaptadores de canal: traducir lo que llega y enviar lo que sale.
//!
//! **El canal es una interfaz desde el primer día**, aunque el primer cliente
//! solo compre WhatsApp. No es una previsión: de las cinco peticiones de Workana
//! que definen el alcance, una pide los tres canales de Meta. Y como los tres
//! comparten consola, token y modelo de webhook, soportarlos no es triple
//! trabajo — es este módulo con dos implementaciones.
//!
//! Lo que cada adaptador hace, y nada más:
//!
//! - **traducir** el cuerpo del webhook a [`MensajeEntrante`], que es lo único
//!   que ve el resto del programa;
//! - **enviar** un texto con las credenciales de la cuenta.
//!
//! Lo que NO hace: decidir qué se responde. Esa regla está en `chasqui-dominio` y
//! es la misma para los tres canales.

pub mod meta_mensajeria;
pub mod web;
pub mod whatsapp;

use async_trait::async_trait;
use chasqui_dominio::{Canal, MensajeEntrante, MensajeSaliente};

/// Lo que puede salir mal al hablar con un canal.
#[derive(Debug, thiserror::Error)]
pub enum ErrorCanal {
    #[error(
        "el cuerpo del webhook de {canal} no tiene la forma esperada: {detalle}. O la app de Meta \
         está suscrita a un campo que Chasqui no atiende, o la versión de la API cambió."
    )]
    Forma { canal: Canal, detalle: String },

    #[error("Meta rechazó el envío por {canal} con {estado}: {cuerpo}")]
    Rechazado { canal: Canal, estado: u16, cuerpo: String },

    #[error("no se pudo hablar con la API Graph: {0}")]
    Red(#[from] reqwest::Error),

    #[error("el canal {canal} no entrega archivos por referencia")]
    SinMedios { canal: Canal },

    #[error(
        "el archivo pesa {tamano} y el tope es {tope}. No se guarda a medias: medio archivo se \
         enseña como un archivo roto y nadie sabría que le faltan bytes."
    )]
    DemasiadoGrande { tamano: u64, tope: usize },
}

/// Un canal por el que se recibe y se envía.
#[async_trait]
pub trait AdaptadorCanal: Send + Sync {
    fn canal(&self) -> Canal;

    /// Traduce el cuerpo del webhook —ya verificado— a mensajes normalizados.
    ///
    /// Lo que no es un mensaje (acuses de entrega, confirmaciones de lectura, el
    /// eco de lo que enviamos nosotros) **no es un error**: se devuelve una lista
    /// más corta. Meta manda esos eventos constantemente y tratarlos como fallo
    /// llenaría el log de ruido hasta que nadie lo mire.
    fn entrantes(&self, cuerpo: &serde_json::Value) -> Result<Vec<MensajeEntrante>, ErrorCanal>;

    /// Envía un texto. Devuelve el acuse de Meta si lo hubo.
    ///
    /// `cuenta` es el id externo de la cuenta que responde, y `token` el del
    /// cliente. Ninguno de los dos se guarda en el adaptador: se pasan en cada
    /// envío para que un token rotado no viva en memoria hasta el próximo
    /// reinicio.
    async fn enviar(
        &self,
        token: &str,
        cuenta: &str,
        mensaje: &MensajeSaliente,
    ) -> Result<Option<String>, ErrorCanal>;

    /// Cuántos caracteres admite un mensaje en este canal.
    ///
    /// No es el mismo en los tres, y pasarse **no trunca: Meta rechaza el envío
    /// entero**. Una respuesta larga de la base de conocimiento se parte en
    /// varios mensajes; ese es el trabajo de [`trozos`].
    fn limite_texto(&self) -> usize;

    /// Trae los bytes de un medio que el canal solo entregó por referencia.
    ///
    /// Lo implementa quien lo tenga: en los canales de Meta son **dos** llamadas
    /// —el identificador da una URL temporal, y esa URL da el archivo—, y la URL
    /// caduca en minutos, por lo que esto se llama al recibir el mensaje y no
    /// cuando alguien abre el panel.
    ///
    /// El defecto es negarse, no devolver vacío: un canal que no trae medios
    /// —el widget web, que sube los suyos por otra puerta— tiene que decirlo, no
    /// entregar cero bytes que después se enseñan como una foto rota.
    async fn descargar_medio(
        &self,
        _token: &str,
        _id: &str,
        _tope: usize,
    ) -> Result<Descarga, ErrorCanal> {
        Err(ErrorCanal::SinMedios { canal: self.canal() })
    }
}

/// Un archivo traído del canal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descarga {
    pub bytes: Vec<u8>,
    /// El que dice la descarga, que manda sobre el que decía el webhook.
    pub mime: Option<String>,
}

/// Comprueba que una URL de descarga apunta a internet y no a la máquina del
/// cliente.
///
/// # Por qué hace falta si el webhook va firmado
///
/// En Instagram y Messenger el «billete» del adjunto **es una URL que viene
/// dentro del cuerpo del webhook**, y Chasqui la pide. La firma HMAC garantiza
/// que el cuerpo lo mandó quien tiene el secreto de la app; no garantiza que la
/// URL apunte a la CDN de Meta. Con `http://` valía `http://127.0.0.1:PUERTO/…`
/// o `http://169.254.169.254/latest/meta-data/`, y el resultado se guarda como
/// un medio que quien atiende puede abrir en el panel: el servidor del cliente
/// pidiéndose a sí mismo lo que hay dentro de su propia red.
///
/// **Lo que esta comprobación cubre y lo que no**, dicho sin adornos: cubre el
/// esquema —solo `https`— y las direcciones IP escritas a la cara que caen en
/// rangos internos. NO resuelve nombres, así que un nombre de la intranet del
/// cliente que apunte a una IP privada seguiría pasando. Cerrar eso exige
/// resolver y comprobar cada dirección antes de conectarse, que es otro
/// mecanismo con su propio peso; esto quita el camino fácil y lo declara.
///
/// # La única excepción, y por qué no es un agujero
///
/// Se admite una URL que esté en **el mismo origen que la API que esta
/// instalación ya tiene configurada** (`CHASQUI_GRAPH`). No es una bandera que
/// alguien pueda encender: sale de la configuración que ya existe, y la pide el
/// llamante al apuntar Chasqui a otra API. Es lo que hace que las pruebas de
/// frontera —que levantan una Graph falsa en `127.0.0.1`— sigan cruzando la
/// frontera del programa de verdad (directiva 28) en vez de tener que rodear la
/// comprobación, que es como una salvaguarda acaba desactivada en producción.
///
/// En una instalación real `CHASQUI_GRAPH` es `https://graph.facebook.com/…`, y
/// entonces la excepción solo alcanza a `graph.facebook.com`, que ya pasaba por
/// la vía estricta: allí no admite ni un destino más.
pub(crate) fn url_de_descarga_admisible(url: &str, base_api: &str) -> Result<(), String> {
    let partido = reqwest::Url::parse(url).map_err(|e| format!("no es una URL ({e}): {url:?}"))?;

    if let Ok(base) = reqwest::Url::parse(base_api)
        && base.host_str().is_some()
        && base.scheme() == partido.scheme()
        && base.host_str() == partido.host_str()
        && base.port_or_known_default() == partido.port_or_known_default()
    {
        return Ok(());
    }

    // Solo `https`. Meta sirve los adjuntos por HTTPS desde hace más de una
    // década, así que esto no le cierra la puerta a ningún cliente legítimo
    // (directiva 21) y sí cierra `http://` hacia una red interna.
    if partido.scheme() != "https" {
        return Err(format!(
            "el esquema es {:?} y solo se admite https: una descarga por http puede apuntar a la \
             red interna del servidor",
            partido.scheme()
        ));
    }

    let Some(anfitrion) = partido.host_str() else {
        return Err(format!("la URL no trae anfitrión: {url:?}"));
    };

    // `[::1]` llega entre corchetes; sin quitarlos no parsea y se colaría.
    let pelado = anfitrion.trim_start_matches('[').trim_end_matches(']');
    if anfitrion.eq_ignore_ascii_case("localhost") {
        return Err("el anfitrión es localhost".to_string());
    }
    if let Ok(ip) = pelado.parse::<std::net::IpAddr>() {
        let interna = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    // 100.64.0.0/10, el NAT de operador. No hay método estable.
                    || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    // fc00::/7 (únicas locales) y fe80::/10 (enlace local).
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
                    // Una IPv4 interna envuelta en IPv6 es la misma IPv4.
                    || v6.to_ipv4_mapped().is_some_and(|v4| {
                        v4.is_loopback() || v4.is_private() || v4.is_link_local()
                    })
            }
        };
        if interna {
            return Err(format!("la dirección {ip} es de la red interna del servidor"));
        }
    }
    Ok(())
}

/// Baja un cuerpo **sin llegar a tenerlo entero en memoria si se pasa del tope**.
///
/// `Response::bytes()` reserva el cuerpo completo y solo después se puede mirar
/// cuánto pesaba: negarse ahí evita guardarlo, no reservarlo. Con una descarga
/// por cada archivo que llega y sin nada que las cuente, un puñado de respuestas
/// grandes a la vez se lleva por delante la memoria del servidor del cliente.
/// Aquí se corta **en cuanto** el acumulado pasa del tope, y antes de empezar si
/// el `Content-Length` ya lo dice.
pub(crate) async fn bajar_con_tope(
    peticion: reqwest::RequestBuilder,
    tope: usize,
) -> Result<Descarga, ErrorCanal> {
    let mut respuesta = peticion.send().await?.error_for_status()?;

    if let Some(largo) = respuesta.content_length()
        && largo > tope as u64
    {
        return Err(ErrorCanal::DemasiadoGrande { tamano: largo, tope });
    }

    let mime = respuesta
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let mut bytes: Vec<u8> = Vec::new();
    while let Some(trozo) = respuesta.chunk().await? {
        bytes.extend_from_slice(&trozo);
        if bytes.len() > tope {
            // Se dice el tamaño que se lleva contado, que es lo que se sabe. No
            // se inventa el total: el cuerpo se abandona sin terminar de leerlo.
            return Err(ErrorCanal::DemasiadoGrande { tamano: bytes.len() as u64, tope });
        }
    }
    Ok(Descarga { bytes, mime })
}

impl ErrorCanal {
    /// Qué hacer, dicho para **quien atiende la cola** y no para quien programa.
    ///
    /// Nace de verlo en pantalla el 2026-08-02: al agente le apareció en la cola
    /// `Meta rechazó el envío con 401: {"error":{"message":"Authentication
    /// Error","code":190,…}}`. Es exacto y es inútil — quien atiende no puede
    /// hacer nada con eso, y lo que necesita saber es que hay que avisar a quien
    /// administra el servidor. El texto crudo NO se tira: se conserva debajo,
    /// porque es lo que sí le sirve a esa otra persona.
    ///
    /// **Solo se traduce lo que significa algo con certeza** (directiva 23).
    /// Inventar una explicación para cualquier error convertiría un diagnóstico
    /// en una adivinanza, y una alarma que dos veces dice lo que no es, a la
    /// tercera no la mira nadie.
    pub fn explicacion(&self) -> String {
        let que_pasa = match self {
            ErrorCanal::Rechazado { estado: 401, .. } | ErrorCanal::Rechazado { estado: 403, .. } => {
                "Las credenciales de WhatsApp ya no sirven: caducaron o se revocaron. \
                 AVISE A QUIEN ADMINISTRA EL SERVIDOR — hay que poner un token nuevo. \
                 Mientras tanto el bot no puede responder a nadie."
            }
            ErrorCanal::Rechazado { estado: 429, .. } => {
                "Meta está limitando los envíos por exceso de mensajes. \
                 Suele pasarse solo; si sigue, avise a quien administra."
            }
            ErrorCanal::Rechazado { estado, .. } if *estado >= 500 => {
                "Meta está fallando de su lado. Vuelva a intentarlo en unos minutos."
            }
            ErrorCanal::Rechazado { .. } => {
                "Meta rechazó el mensaje. Responda por otra vía y avise a quien administra."
            }
            ErrorCanal::Red(_) => {
                "No se pudo hablar con Meta: puede ser la conexión del servidor. \
                 Si se repite, avise a quien administra."
            }
            // Un fallo de forma no ocurre al ENVIAR, y por eso no se le inventa
            // una recomendación: aquí solo se dice qué pasó.
            ErrorCanal::Forma { .. } | ErrorCanal::SinMedios { .. } | ErrorCanal::DemasiadoGrande { .. } => {
                "El mensaje no se pudo enviar."
            }
        };
        // El detalle va DEBAJO y completo: quien administra lo necesita entero
        // para saber si es el token, el permiso o la red.
        format!("{que_pasa}\n\nDetalle técnico: {self}")
    }
}

/// Parte un texto en trozos que quepan en el canal, cortando por líneas o
/// espacios cuando se puede.
///
/// Partir y no truncar: la mitad de una respuesta parece una respuesta, y el
/// cliente nunca se entera de que faltaba lo importante. Es un texto que se corta
/// mal, no un dato que se sustituye — el fallo se ve.
pub fn trozos(texto: &str, limite: usize) -> Vec<String> {
    assert!(limite > 0, "el límite de un canal no puede ser cero");

    let mut partes = Vec::new();
    let mut resto = texto.trim();

    while !resto.is_empty() {
        if resto.chars().count() <= limite {
            partes.push(resto.to_string());
            break;
        }

        // El corte tiene que caer en un borde de carácter: un `&texto[..limite]`
        // sobre bytes revienta en cuanto hay una tilde, y los hay en todas las
        // frases en español.
        let corte_max = resto
            .char_indices()
            .nth(limite)
            .map(|(i, _)| i)
            .unwrap_or(resto.len());
        let ventana = &resto[..corte_max];

        let corte = ventana
            .rfind('\n')
            .or_else(|| ventana.rfind(' '))
            // Sin ningún espacio (una URL larguísima) se corta donde toque: es
            // feo y es lo correcto, porque lo otro es no enviar nada.
            .unwrap_or(corte_max);

        let (parte, siguiente) = resto.split_at(corte);
        partes.push(parte.trim_end().to_string());
        resto = siguiente.trim_start();
    }

    if partes.is_empty() {
        partes.push(String::new());
    }
    partes
}

#[cfg(test)]
mod pruebas_explicacion {
    use super::*;
    use chasqui_dominio::Canal;

    fn rechazo(estado: u16) -> ErrorCanal {
        ErrorCanal::Rechazado {
            canal: Canal::WhatsApp,
            estado,
            cuerpo: r#"{"error":{"message":"Authentication Error","code":190}}"#.into(),
        }
    }

    #[test]
    fn el_token_caducado_le_dice_a_quien_atiende_que_hacer() {
        // El caso exacto que se vio en la cola el 2026-08-02.
        let e = rechazo(401).explicacion();
        assert!(e.contains("credenciales"), "{e}");
        assert!(e.to_lowercase().contains("avise"), "no dice a quién acudir: {e}");
        // Y el crudo sigue ahí para quien administra.
        assert!(e.contains("code\":190"), "se perdió el detalle técnico: {e}");
    }

    #[test]
    fn ningun_fallo_empieza_por_un_json() {
        // La regla, no el caso: lo primero que lee una persona nunca puede ser
        // el volcado de un error de una API.
        for e in [
            rechazo(401),
            rechazo(429),
            rechazo(500),
            rechazo(418),
            ErrorCanal::SinMedios { canal: Canal::Web },
            ErrorCanal::DemasiadoGrande { tamano: 99, tope: 10 },
            ErrorCanal::Forma { canal: Canal::WhatsApp, detalle: "raro".into() },
        ] {
            let texto = e.explicacion();
            let primera = texto.lines().next().unwrap_or_default();
            assert!(!primera.trim_start().starts_with('{'), "empieza con JSON: {primera}");
            assert!(!primera.contains("code"), "empieza con jerga: {primera}");
            assert!(primera.ends_with('.') || primera.ends_with(' '), "no es una frase: {primera}");
        }
    }

    #[test]
    fn un_limite_de_envios_no_se_confunde_con_un_token_muerto() {
        // Si los dos dijeran «avise al administrador», la alarma que importa se
        // perdería entre las que se pasan solas.
        let limite = rechazo(429).explicacion();
        assert!(limite.contains("limitando"), "{limite}");
        assert!(!limite.contains("credenciales"), "confundió un 429 con un 401: {limite}");
    }
}

#[cfg(test)]
mod pruebas_url_de_descarga {
    /// La API de una instalación de verdad. Todas las pruebas de aquí miden
    /// contra ella: la excepción del mismo origen tiene su propia prueba abajo.
    const GRAPH: &str = "https://graph.facebook.com/v21.0";

    fn admisible(url: &str) -> Result<(), String> {
        super::url_de_descarga_admisible(url, GRAPH)
    }

    #[test]
    fn las_urls_de_verdad_de_meta_pasan() {
        // LA MITAD QUE IMPIDE SOBRECORREGIR (directiva 21). Una regla que
        // rechaza lo interno y también lo bueno es una regla que hay que quitar
        // el primer día que un cliente manda una foto. Estas son las formas
        // reales que sirven los adjuntos de WhatsApp, Instagram y Messenger.
        for url in [
            "https://lookaside.fbsbx.com/whatsapp_business/attachments/?mid=ABC&ext=1&hash=xyz",
            "https://scontent.xx.fbcdn.net/v/t1.0-9/12345_n.jpg?_nc_cat=1&oh=abc&oe=DEF",
            "https://mmg.whatsapp.net/d/f/Aq1.enc",
            "https://graph.facebook.com/v21.0/1234567890",
        ] {
            assert!(admisible(url).is_ok(), "rechazó una URL buena: {url} → {:?}", admisible(url));
        }
    }

    #[test]
    fn el_http_a_secas_no_pasa_aunque_el_anfitrion_sea_de_meta() {
        // La regla es el ESQUEMA, no el destino: comprobar solo el anfitrión
        // dejaría pasar cualquier http que casara con una lista, y una lista de
        // dominios de Meta caduca sola.
        let r = admisible("http://lookaside.fbsbx.com/x.jpg");
        assert!(r.is_err(), "aceptó http hacia Meta");
        assert!(r.unwrap_err().contains("https"), "el motivo no dice cuál es la regla");
    }

    #[test]
    fn nada_apunta_a_la_maquina_ni_a_la_red_del_cliente() {
        // El ataque completo: quien tenga el secreto de la app del cliente mete
        // esta URL en el webhook y Chasqui le trae lo que hay dentro de su red,
        // guardado como un archivo que se abre desde el panel.
        for url in [
            "http://127.0.0.1:8080/admin",
            "https://127.0.0.1:8080/admin",
            "http://localhost/",
            "https://LOCALHOST/x",
            "http://169.254.169.254/latest/meta-data/iam/security-credentials/",
            "https://169.254.169.254/latest/meta-data/",
            "https://10.0.0.5/interno",
            "https://192.168.1.1/router",
            "https://172.16.30.9/",
            "https://100.64.0.1/",
            "https://0.0.0.0/",
            "https://[::1]/",
            "https://[fe80::1]/",
            "https://[fd00::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            assert!(admisible(url).is_err(), "dejó pasar una URL hacia dentro: {url}");
        }
    }

    #[test]
    fn una_ip_publica_si_pasa() {
        // Que discrimine por RANGO y no por «es una IP»: hay CDNs que sirven por
        // dirección, y rechazarlas todas sería la sobrecorrección de siempre.
        assert!(admisible("https://8.8.8.8/x").is_ok());
        assert!(admisible("https://[2001:4860:4860::8888]/x").is_ok());
    }

    #[test]
    fn la_excepcion_del_mismo_origen_no_ensancha_nada_en_produccion() {
        // Con la API de verdad configurada, la excepción solo alcanza a la
        // propia graph.facebook.com —que ya pasaba— y a nada más. Si esto se
        // cae, la excepción dejó de ser un no-op y se convirtió en una puerta.
        assert!(super::url_de_descarga_admisible("https://graph.facebook.com/v21.0/1", GRAPH).is_ok());
        for interno in ["http://127.0.0.1:9/x", "https://169.254.169.254/x", "http://graph.facebook.com/x"] {
            assert!(
                super::url_de_descarga_admisible(interno, GRAPH).is_err(),
                "la excepción dejó pasar {interno} con la API real configurada"
            );
        }
    }

    #[test]
    fn una_api_local_configurada_a_proposito_si_alcanza_a_su_propio_origen() {
        // Es el caso de las pruebas de frontera y de `probar-con-meta.sh`: una
        // Graph falsa en el bucle local. Lo pide la configuración del llamante,
        // no una bandera suelta — y sigue sin alcanzar a NINGÚN otro destino
        // interno, que es lo que la hace una excepción y no un apagado.
        let local = "http://127.0.0.1:8123/v21.0";
        assert!(super::url_de_descarga_admisible("http://127.0.0.1:8123/descargas/1", local).is_ok());
        for otro in ["http://127.0.0.1:9999/x", "http://169.254.169.254/x", "http://192.168.1.1/x"] {
            assert!(
                super::url_de_descarga_admisible(otro, local).is_err(),
                "con una Graph local, {otro} tampoco puede pasar"
            );
        }
    }

    #[test]
    fn lo_que_ni_siquiera_es_una_url_se_rechaza_diciendo_que_no_lo_es() {
        for basura in ["", "MEDIA-123", "ftp://x/y", "javascript:alert(1)", "file:///etc/passwd"] {
            assert!(admisible(basura).is_err(), "aceptó {basura:?}");
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn un_texto_corto_sale_de_una_pieza() {
        assert_eq!(trozos("hola", 100), vec!["hola".to_string()]);
    }

    #[test]
    fn un_texto_largo_se_parte_sin_perder_nada() {
        let texto = "palabra ".repeat(50);
        let partes = trozos(&texto, 40);
        assert!(partes.len() > 1);
        for p in &partes {
            assert!(p.chars().count() <= 40, "un trozo se pasó del límite: {p:?}");
        }
        // Lo importante: nada se perdió por el camino.
        let rearmado = partes.join(" ");
        assert_eq!(rearmado.split_whitespace().count(), 50);
    }

    #[test]
    fn corta_por_el_salto_de_linea_cuando_lo_hay() {
        let partes = trozos("primera línea\nsegunda línea", 20);
        assert_eq!(partes[0], "primera línea");
    }

    #[test]
    fn no_revienta_con_tildes_en_el_borde() {
        // El caso rojo del corte por bytes: «ñ» ocupa dos bytes y cortar en el
        // primero es un pánico en tiempo de ejecución, no un texto feo.
        let texto = "ñ".repeat(200);
        let partes = trozos(&texto, 10);
        assert!(partes.iter().all(|p| p.chars().count() <= 10));
        assert_eq!(partes.concat().chars().count(), 200);
    }

    #[test]
    fn una_palabra_mas_larga_que_el_limite_se_corta_igual() {
        let largo = "a".repeat(25);
        let partes = trozos(&largo, 10);
        assert_eq!(partes.len(), 3);
        assert_eq!(partes.concat(), largo);
    }

    #[test]
    fn un_texto_vacio_da_un_trozo_vacio_y_no_un_bucle() {
        assert_eq!(trozos("", 10), vec![String::new()]);
        assert_eq!(trozos("   ", 10), vec![String::new()]);
    }
}
