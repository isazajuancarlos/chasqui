// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! El mensaje **normalizado**: lo que queda de un webhook de Meta una vez se le
//! quita la forma de cada canal.
//!
//! Todo lo que hay debajo de la traducción trabaja con estos tipos. Es lo que
//! hace que soportar los tres canales no sea triple trabajo: la atención no sabe
//! si el mensaje vino de un `wa_id` o de un PSID.

use chrono::{DateTime, Utc};

use crate::canal::Canal;

/// Qué clase de mensaje llegó.
///
/// `NoSoportado` es deliberado: **una foto o un audio no se convierten en texto
/// vacío**. Un texto vacío se responde con el guion de «no entendí» y el cliente
/// nunca se entera de que le mandaron una nota de voz; nombrarlo deja que la
/// atención decida escalar a una persona, que es lo correcto.
#[derive(Debug, Clone, PartialEq)]
pub enum Clase {
    Texto(String),
    /// Botón o respuesta de lista: trae el identificador que el cliente configuró.
    Interactivo { id: String, titulo: String },
    /// Una ubicación compartida. **No hay nada que descargar**: las coordenadas
    /// vienen en el propio webhook, así que tirarlas era regalar un dato que ya
    /// nos habían dado.
    Ubicacion(Ubicacion),
    /// Una foto, una nota de voz, un vídeo, un documento. **Aquí solo viaja el
    /// billete**: Meta manda un identificador, no los bytes, y bajarlos es cosa
    /// de la capa que habla por la red. El dominio no descarga nada.
    Medio(Medio),
    /// Llegó algo que este canal admite y nosotros todavía no. Guarda el nombre
    /// que le da Meta, que es lo que hace falta para soportarlo mañana.
    NoSoportado(String),
}

/// Dónde está quien escribe, tal como lo mandó.
///
/// Las coordenadas son `f64` y no cadenas: son números, y quien las reciba va a
/// querer calcular con ellas —una distancia, una zona de reparto—. El precio es
/// que `Clase` ya no puede derivar `Eq`, que en un `f64` no significaría nada
/// útil de todos modos.
#[derive(Debug, Clone, PartialEq)]
pub struct Ubicacion {
    pub latitud: f64,
    pub longitud: f64,
    /// Cómo llama Meta al sitio, si lo sabe. Es opcional de verdad: compartir la
    /// ubicación en vivo no trae nombre.
    pub nombre: Option<String>,
    pub direccion: Option<String>,
}

/// El billete de un archivo que está en los servidores de Meta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Medio {
    /// `image`, `audio`, `video`, `document`, `sticker`: el nombre de Meta.
    pub tipo: String,
    /// El identificador con el que se pide el archivo. **Sin esto el archivo se
    /// pierde para siempre**, y esa fue exactamente la fuga que se tapó.
    pub id: String,
    pub mime: Option<String>,
    /// El nombre que le puso quien lo mandó. Solo los documentos lo traen.
    pub nombre_archivo: Option<String>,
}

impl Clase {
    /// El texto sobre el que se busca en la base de conocimiento. `None` cuando
    /// no hay nada que buscar.
    pub fn texto(&self) -> Option<&str> {
        match self {
            Clase::Texto(t) => Some(t),
            Clase::Interactivo { titulo, .. } => Some(titulo),
            // Ni una ubicación ni una foto se buscan en la base de conocimiento:
            // no hay pregunta que casar, y casar una dirección contra las
            // preguntas frecuentes daría una respuesta al azar. Van a una
            // persona, que es quien puede mirarlas.
            Clase::Ubicacion(_) | Clase::Medio(_) | Clase::NoSoportado(_) => None,
        }
    }

    /// Cómo se le nombra esto **a la persona que atiende**, en el panel y en el
    /// historial.
    ///
    /// Existe porque el 2026-08-02, atendiendo un WhatsApp de verdad, al agente
    /// le apareció `[NoSoportado("audio")]` en pantalla: el `Debug` de un enum
    /// de Rust servido de cara al cliente. Quien atiende no tiene por qué saber
    /// qué es un enum, y con eso escrito **no sabe siquiera que le mandaron una
    /// nota de voz**.
    ///
    /// Los corchetes se conservan a propósito: marcan que la frase la puso el
    /// sistema y no la escribió el cliente, que perfectamente podría teclear
    /// «una foto».
    pub fn descripcion(&self) -> String {
        match self {
            Clase::Texto(t) => t.clone(),
            Clase::Interactivo { titulo, .. } => titulo.clone(),
            Clase::Ubicacion(u) => u.descripcion(),
            Clase::Medio(m) => describir_no_soportado(&m.tipo),
            Clase::NoSoportado(tipo) => describir_no_soportado(tipo),
        }
    }
}

impl Ubicacion {
    /// Lo que ve quien atiende. Lleva **las coordenadas en claro**: son el dato,
    /// y un nombre de sitio sin ellas no sirve para llevarle nada a nadie.
    ///
    /// No se compone aquí ningún enlace a un mapa: eso es una decisión de
    /// presentación y de qué proveedor usa el cliente, y el dominio no elige
    /// proveedor por él.
    pub fn descripcion(&self) -> String {
        let mut partes = Vec::new();

        // Lo que dijo quien escribe, si dijo algo.
        match (&self.nombre, &self.direccion) {
            (Some(n), Some(d)) if n != d => partes.push(format!("{n} — {d}")),
            (Some(t), _) | (_, Some(t)) => partes.push(t.clone()),
            (None, None) => {}
        }

        // Dónde cae, resuelto SIN salir de esta máquina (ver `geo`).
        //
        // Se escribe «a 1,6 km de Amagá» y no «Amagá (a 1,6 km)» a propósito: lo
        // segundo se puede leer como «a 1,6 km de las afueras de Amagá», y en
        // una ciudad grande el punto de referencia está a varios kilómetros de
        // barrios que sí son la ciudad. La primera forma dice exactamente lo que
        // se sabe —la distancia a un punto conocido— sin afirmar si está dentro.
        if let Some(lugar) = crate::geo::situar(self.latitud, self.longitud) {
            let km = format!("{:.1}", lugar.km).replace('.', ",");
            partes.push(format!("a {km} km de {}", lugar.nombre()));
        }

        // Las coordenadas van SIEMPRE y las últimas. Son el dato con el que se
        // opera —a una casa se llega con un punto, no con un municipio—, y la
        // etiqueta de arriba es contexto para entenderlo de un vistazo, no su
        // sustituto.
        partes.push(format!("{}, {}", self.latitud, self.longitud));

        format!("[una ubicación: {}]", partes.join(" · "))
    }
}

/// El nombre humano de lo que llegó y no se sabe leer.
///
/// **Solo se traduce lo que significa algo con certeza**; para todo lo demás se
/// dice el tipo tal cual y se admite que no se sabe, en vez de inventarle un
/// nombre bonito (directiva 23). Un `NoSoportado` no siempre trae un tipo de
/// medio: también recoge diagnósticos como «text sin body», y una frase
/// genérica que los tape los volvería invisibles.
fn describir_no_soportado(tipo: &str) -> String {
    // Messenger e Instagram nombran el adjunto `adjunto:image`; WhatsApp manda
    // `image` a secas. Es el mismo hecho con dos vocabularios, y el sitio de
    // reconciliarlos es aquí —donde se lee— y no en cada canal.
    let tipo = tipo.strip_prefix("adjunto:").unwrap_or(tipo);
    let nombre = match tipo {
        "audio" => "una nota de voz",
        "image" => "una foto",
        "video" => "un vídeo",
        "document" => "un documento",
        "location" => "una ubicación",
        "sticker" => "un sticker",
        "contacts" => "un contacto",
        _ => return format!("[algo que este bot no sabe leer: «{tipo}»]"),
    };
    format!("[{nombre}]")
}

/// Un mensaje que entra, ya traducido del dialecto de su canal.
///
/// Sin `Eq` desde que una ubicación trae coordenadas: comparar dos `f64` por
/// igualdad exacta no significa nada, y fingir que sí con `Eq` sería peor que
/// no tenerlo. `PartialEq` basta para las pruebas, que es donde se usa.
#[derive(Debug, Clone, PartialEq)]
pub struct MensajeEntrante {
    pub canal: Canal,
    /// La cuenta NUESTRA que lo recibió: `phone_number_id` en WhatsApp, el id de
    /// la página o de la cuenta profesional en Messenger e Instagram. Es la que
    /// dice con qué credenciales se contesta.
    pub cuenta: String,
    /// Quién escribe: `wa_id`, PSID o IGSID. Es un dato personal.
    pub contacto: String,
    /// El identificador que le puso Meta. **Es la clave de la idempotencia**:
    /// Meta reintenta el webhook si no recibe un 200 a tiempo, y sin esto el
    /// cliente recibe la misma respuesta tres veces.
    pub id_externo: String,
    pub clase: Clase,
    pub momento: DateTime<Utc>,
}

/// Un mensaje que sale. El canal y las credenciales los pone quien lo envía.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MensajeSaliente {
    /// A quién: el mismo identificador con el que llegó el entrante.
    pub para: String,
    pub texto: String,
}

impl MensajeSaliente {
    pub fn nuevo(para: impl Into<String>, texto: impl Into<String>) -> Self {
        MensajeSaliente { para: para.into(), texto: texto.into() }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn lo_no_soportado_no_se_degrada_a_texto_vacio() {
        // Si esto devolviera Some(""), una nota de voz se respondería con «no
        // entendí» y nadie sabría que hubo una nota de voz.
        assert_eq!(Clase::NoSoportado("audio".into()).texto(), None);
    }

    #[test]
    fn el_panel_nunca_ve_el_debug_de_un_enum() {
        // Lo que se rompió de verdad el 2026-08-02: en el panel salía
        // `[NoSoportado("audio")]`. Esta prueba se cae si alguien vuelve a
        // guardar el `Debug` de la clase.
        for tipo in ["audio", "image", "adjunto:image", "location", "cualquiera"] {
            let d = Clase::NoSoportado(tipo.into()).descripcion();
            assert!(!d.contains("NoSoportado"), "se escapó el enum: {d}");
            assert!(!d.contains('\"'), "se escapó el Debug: {d}");
        }
    }

    #[test]
    fn los_medios_conocidos_se_nombran_en_castellano() {
        assert_eq!(Clase::NoSoportado("audio".into()).descripcion(), "[una nota de voz]");
        assert_eq!(Clase::NoSoportado("image".into()).descripcion(), "[una foto]");
        // Messenger e Instagram lo nombran con prefijo: mismo hecho, misma frase.
        assert_eq!(Clase::NoSoportado("adjunto:image".into()).descripcion(), "[una foto]");
    }

    #[test]
    fn lo_desconocido_dice_el_tipo_y_admite_que_no_lo_sabe() {
        // Aquí es donde una frase genérica —«un archivo»— haría daño: taparía
        // el nombre que hace falta para soportarlo mañana, y taparía también
        // los diagnósticos, que no son medios.
        let d = Clase::NoSoportado("text sin body".into()).descripcion();
        assert!(d.contains("text sin body"), "perdió el tipo: {d}");
        assert!(d.contains("no sabe leer"), "no admite que no lo sabe: {d}");
    }

    fn ubicacion(nombre: Option<&str>, direccion: Option<&str>) -> Clase {
        Clase::Ubicacion(Ubicacion {
            latitud: 6.038,
            longitud: -75.702,
            nombre: nombre.map(str::to_string),
            direccion: direccion.map(str::to_string),
        })
    }

    #[test]
    fn una_ubicacion_siempre_enseña_las_coordenadas() {
        // Un nombre de sitio sin coordenadas no sirve para llevarle nada a
        // nadie, y «Casa» es el nombre que le pone medio mundo a su ubicación.
        for clase in [
            ubicacion(Some("Casa"), Some("Cra 50 #10-20")),
            ubicacion(Some("Casa"), None),
            ubicacion(None, None),
        ] {
            let d = clase.descripcion();
            assert!(d.contains("6.038"), "perdió la latitud: {d}");
            assert!(d.contains("-75.702"), "perdió la longitud: {d}");
        }
    }

    #[test]
    fn una_ubicacion_sin_nombre_no_deja_un_hueco_ni_un_guion_suelto() {
        // Compartir la ubicación en vivo no manda nombre. El texto tiene que
        // seguir leyéndose como una frase, no como una plantilla a medio rellenar.
        let d = ubicacion(None, None).descripcion();
        assert!(d.starts_with("[una ubicación: a "), "{d}");
        assert!(!d.contains("·  ·"), "quedó un hueco: {d}");
        assert!(!d.contains(": ·"), "empieza con un separador suelto: {d}");
    }

    #[test]
    fn una_ubicacion_dice_el_municipio_sin_preguntarle_a_ningun_mapa() {
        // La etiqueta se resuelve dentro de la máquina; ver `geo`. Aquí se fija
        // que llega hasta el texto que ve quien atiende.
        let d = ubicacion(None, None).descripcion();
        assert!(d.contains("Amagá"), "no situó el punto: {d}");
        assert!(d.contains("Antioquia") && d.contains("Colombia"), "{d}");
    }

    #[test]
    fn la_etiqueta_no_sustituye_a_las_coordenadas() {
        // A una casa se llega con un punto, no con un municipio. Si algún día
        // alguien decide que el nombre basta, esto se cae.
        let d = ubicacion(Some("Casa"), None).descripcion();
        assert!(d.contains("6.038") && d.contains("-75.702"), "perdió el dato: {d}");
        assert!(d.contains("Casa"), "perdió lo que dijo el cliente: {d}");
    }

    #[test]
    fn en_mitad_del_oceano_la_ubicacion_sale_solo_con_sus_coordenadas() {
        let clase = Clase::Ubicacion(Ubicacion {
            latitud: -30.0,
            longitud: -140.0,
            nombre: None,
            direccion: None,
        });
        assert_eq!(clase.descripcion(), "[una ubicación: -30, -140]");
    }

    #[test]
    fn una_ubicacion_no_repite_el_nombre_cuando_es_igual_a_la_direccion() {
        let d = ubicacion(Some("Cra 50 #10-20"), Some("Cra 50 #10-20")).descripcion();
        assert_eq!(d.matches("Cra 50 #10-20").count(), 1, "lo dijo dos veces: {d}");
    }

    #[test]
    fn un_medio_se_nombra_por_lo_que_es() {
        let foto = Clase::Medio(Medio {
            tipo: "image".into(),
            id: "MEDIA-1".into(),
            mime: Some("image/jpeg".into()),
            nombre_archivo: None,
        });
        assert_eq!(foto.descripcion(), "[una foto]");
        // Y sigue sin haber nada que buscar en la base de conocimiento: quien
        // puede mirar una foto es una persona.
        assert_eq!(foto.texto(), None);
    }

    #[test]
    fn un_texto_se_describe_tal_cual_y_sin_corchetes() {
        // Los corchetes marcan «esto lo puso el sistema». Si se los pusiéramos
        // al texto del cliente, dejarían de significar eso.
        assert_eq!(Clase::Texto("una foto".into()).descripcion(), "una foto");
        assert_eq!(
            Clase::Interactivo { id: "b1".into(), titulo: "Sí".into() }.descripcion(),
            "Sí"
        );
    }

    #[test]
    fn un_interactivo_se_busca_por_su_titulo() {
        let c = Clase::Interactivo { id: "menu_precios".into(), titulo: "Precios".into() };
        assert_eq!(c.texto(), Some("Precios"));
    }
}
