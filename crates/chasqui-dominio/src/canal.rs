// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Los canales por los que entra y sale un mensaje.
//!
//! Tres son de Meta: misma consola de desarrollador, mismo modelo de webhook y de
//! token, endpoints distintos. Por eso el canal es un tipo cerrado desde el primer
//! día aunque al principio solo se venda WhatsApp — añadir Instagram no es
//! reescribir el flujo, es una rama más aquí y un adaptador de salida.
//!
//! **El cuarto, `Web`, no es de Meta y por eso vale la pena** (costura 1 de
//! `docs/INTEGRACION.md`): el chat del sitio del cliente y su aplicación móvil son
//! el MISMO canal —una API y dos consumidores—, y es el único que funciona el
//! primer día, sin app de Meta, sin verificación de negocio, sin token que caduque
//! y sin coste por conversación. Eso convierte «le entrego cuando Meta le
//! apruebe» en «le entrego ya».
//!
//! Y no encaja en los supuestos de los otros tres, lo cual está bien que se note:
//! no hay webhook que Meta firme, no hay número de teléfono que sirva de
//! identidad, y la respuesta no se «envía» a ninguna API ajena. Por eso
//! [`Canal::objeto_meta`] devuelve `Option`: obliga a decidir qué se hace con el
//! canal que no tiene objeto en vez de dejar que herede uno vacío.

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
    /// El chat del sitio del cliente, y su aplicación móvil. No hay webhook: el
    /// navegador habla directamente con Chasqui.
    Web,
}

impl Canal {
    /// Todos los canales. Para recorrerlos sin olvidar ninguno cuando se añada uno.
    pub const TODOS: [Canal; 4] =
        [Canal::WhatsApp, Canal::Instagram, Canal::Messenger, Canal::Web];

    /// Los que llegan por un webhook firmado de Meta.
    ///
    /// Existe para que `/webhook/{canal}` no acepte `web`: ahí no hay nada que
    /// Meta firme, y una ruta que acepta un cuerpo sin firma porque el canal «no
    /// tiene» secreto es exactamente el agujero que la firma existe para tapar.
    pub const DE_META: [Canal; 3] = [Canal::WhatsApp, Canal::Instagram, Canal::Messenger];

    /// ¿Llega por un webhook de Meta?
    pub fn es_de_meta(&self) -> bool {
        self.objeto_meta().is_some()
    }

    /// El nombre en minúsculas: el de la ruta del webhook y el de la columna
    /// `canal` en la base. Uno solo, para que no puedan divergir.
    pub fn ranura(&self) -> &'static str {
        match self {
            Canal::WhatsApp => "whatsapp",
            Canal::Instagram => "instagram",
            Canal::Messenger => "messenger",
            Canal::Web => "web",
        }
    }

    /// El `object` que Meta pone en la raíz del cuerpo del webhook.
    ///
    /// No coincide con la ranura en Messenger —Meta lo llama `page`— y ese
    /// desajuste es justo la clase de detalle que, escrito a mano en dos sitios,
    /// se desincroniza.
    /// `None` en el canal web, que no llega por Meta. Devolver una cadena vacía
    /// habría hecho que `/webhook/web` casara con un cuerpo sin `object`.
    pub fn objeto_meta(&self) -> Option<&'static str> {
        match self {
            Canal::WhatsApp => Some("whatsapp_business_account"),
            Canal::Instagram => Some("instagram"),
            Canal::Messenger => Some("page"),
            Canal::Web => None,
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
        assert_eq!(Canal::Messenger.objeto_meta(), Some("page"));
    }

    #[test]
    fn el_canal_web_no_tiene_objeto_de_meta_y_no_se_le_inventa_uno() {
        // Si `objeto_meta` devolviera "" para web, un cuerpo sin `object` casaría
        // con él y `/webhook/web` aceptaría lo que le mandaran SIN FIRMA.
        assert_eq!(Canal::Web.objeto_meta(), None);
        assert!(!Canal::Web.es_de_meta());
        for canal in Canal::DE_META {
            assert!(canal.es_de_meta(), "{canal} tendría que llegar por webhook");
        }
    }

    #[test]
    fn los_de_meta_son_un_subconjunto_de_todos() {
        // Dos listas escritas a mano se desincronizan. Esto lo caza.
        for canal in Canal::DE_META {
            assert!(Canal::TODOS.contains(&canal));
        }
        assert_eq!(
            Canal::TODOS.iter().filter(|c| c.es_de_meta()).count(),
            Canal::DE_META.len()
        );
    }
}
