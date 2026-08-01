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
    fn un_interactivo_se_busca_por_su_titulo() {
        let c = Clase::Interactivo { id: "menu_precios".into(), titulo: "Precios".into() };
        assert_eq!(c.texto(), Some("Precios"));
    }
}
