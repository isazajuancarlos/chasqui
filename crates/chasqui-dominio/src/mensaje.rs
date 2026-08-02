// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clase {
    Texto(String),
    /// Botón o respuesta de lista: trae el identificador que el cliente configuró.
    Interactivo { id: String, titulo: String },
    /// Llegó algo que este canal admite y nosotros todavía no: imagen, audio,
    /// ubicación, documento. Guarda el nombre que le da Meta.
    NoSoportado(String),
}

impl Clase {
    /// El texto sobre el que se busca en la base de conocimiento. `None` cuando
    /// no hay nada que buscar.
    pub fn texto(&self) -> Option<&str> {
        match self {
            Clase::Texto(t) => Some(t),
            Clase::Interactivo { titulo, .. } => Some(titulo),
            Clase::NoSoportado(_) => None,
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
            Clase::NoSoportado(tipo) => describir_no_soportado(tipo),
        }
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
