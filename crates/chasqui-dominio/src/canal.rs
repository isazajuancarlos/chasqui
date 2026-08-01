// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Los canales por los que entra y sale un mensaje.
//!
//! Los tres son de Meta: misma consola de desarrollador, mismo modelo de webhook
//! y de token, endpoints distintos. Por eso el canal es un tipo cerrado desde el
//! primer día aunque al principio solo se venda WhatsApp — añadir Instagram no es
//! reescribir el flujo, es una rama más aquí y un adaptador de salida.

use std::fmt;

/// Un canal de mensajería. Cerrado a propósito: **no hay `Otro(String)`**.
///
/// Un canal desconocido no es un canal degradado, es una ruta que no sabemos
/// atender; que el tipo no lo pueda representar obliga a decidirlo al añadirlo
/// (directiva 20: ante la ausencia, fallar, no rellenar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Canal {
    WhatsApp,
    Instagram,
    Messenger,
}

impl Canal {
    /// Todos los canales. Para recorrerlos sin olvidar ninguno cuando se añada uno.
    pub const TODOS: [Canal; 3] = [Canal::WhatsApp, Canal::Instagram, Canal::Messenger];

    /// El nombre en minúsculas: el de la ruta del webhook y el de la columna
    /// `canal` en la base. Uno solo, para que no puedan divergir.
    pub fn ranura(&self) -> &'static str {
        match self {
            Canal::WhatsApp => "whatsapp",
            Canal::Instagram => "instagram",
            Canal::Messenger => "messenger",
        }
    }

    /// El `object` que Meta pone en la raíz del cuerpo del webhook.
    ///
    /// No coincide con la ranura en Messenger —Meta lo llama `page`— y ese
    /// desajuste es justo la clase de detalle que, escrito a mano en dos sitios,
    /// se desincroniza.
    pub fn objeto_meta(&self) -> &'static str {
        match self {
            Canal::WhatsApp => "whatsapp_business_account",
            Canal::Instagram => "instagram",
            Canal::Messenger => "page",
        }
    }

    /// Lee un canal de su ranura. `None` si no es ninguno de los tres.
    pub fn desde_ranura(s: &str) -> Option<Canal> {
        Canal::TODOS.into_iter().find(|c| c.ranura() == s)
    }
}

impl fmt::Display for Canal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ranura())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_ranura_y_la_lectura_son_la_misma_operacion_al_derecho_y_al_reves() {
        for canal in Canal::TODOS {
            assert_eq!(Canal::desde_ranura(canal.ranura()), Some(canal));
        }
    }

    #[test]
    fn una_ranura_desconocida_no_se_inventa_un_canal() {
        // El caso rojo de la directiva 20: sin defecto silencioso.
        assert_eq!(Canal::desde_ranura("telegram"), None);
        assert_eq!(Canal::desde_ranura("WhatsApp"), None, "distingue mayúsculas");
        assert_eq!(Canal::desde_ranura(""), None);
    }

    #[test]
    fn messenger_no_se_llama_igual_en_meta_que_en_la_ruta() {
        // La trampa que motiva `objeto_meta`: si alguien la deduce de la ranura,
        // los webhooks de Messenger se descartan por «objeto desconocido».
        assert_eq!(Canal::Messenger.ranura(), "messenger");
        assert_eq!(Canal::Messenger.objeto_meta(), "page");
    }
}
