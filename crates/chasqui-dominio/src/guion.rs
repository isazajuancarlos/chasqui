// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! El guion: los textos fijos que dice el bot y las palabras que lo hacen llamar
//! a una persona.
//!
//! Todo esto lo configura el cliente. Está junto en un tipo, y no repartido por
//! el código, porque es exactamente la superficie que el panel tiene que editar:
//! si un texto vive en un `format!` perdido, el cliente no puede cambiarlo y hay
//! que tocar el binario para corregir una coma.
//!
//! **El aviso de privacidad no tiene valor por defecto.** Ver [`Guion::validar`].

/// Los textos y las palabras clave configurables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guion {
    /// Se manda al abrir cada conversación, antes de cualquier respuesta.
    /// Ley 1581 de 2012: quien trata datos personales avisa.
    pub aviso_privacidad: String,
    /// Lo que dice cuando no encuentra respuesta, justo antes de llamar a alguien.
    pub sin_respuesta: String,
    /// Lo que dice al pasar la conversación a una persona.
    pub escalando: String,
    /// Frases con las que quien escribe pide una persona explícitamente.
    pub palabras_escalado: Vec<String>,
    /// Frases con las que se sale de un flujo de varios pasos.
    ///
    /// **Solo significan algo DENTRO de un flujo.** Fuera, «cancelar» es una
    /// palabra más de una frase: tratarla como orden rompería a quien pregunta
    /// «¿puedo cancelar un pedido?», que es la pregunta más normal del mundo en
    /// un chat de atención (directiva 21).
    pub palabras_cancelar: Vec<String>,
    /// Lo que dice al salirse de un flujo.
    pub cancelado: String,
    /// Se añade a la PRIMERA pregunta de un flujo, para anunciar la salida.
    ///
    /// Vacío lo apaga. Existe porque un flujo captura la conversación —un paso de
    /// texto libre se queda con lo que llegue— y una captura sin salida anunciada
    /// es el bot que el cliente apaga a las dos semanas.
    pub aviso_flujo: String,
}

/// Por qué un guion no sirve. Cada variante es una salida distinta a propósito:
/// «el guion está mal» no le dice al cliente qué arreglar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorGuion {
    #[error(
        "falta el aviso de privacidad. No hay texto por defecto a propósito: el bot trata datos \
         personales y el responsable del tratamiento es el cliente, no nosotros. Un aviso \
         genérico escrito por el proveedor no identifica al responsable y no cumple la Ley 1581."
    )]
    AvisoAusente,
    #[error("falta el texto «{0}»: el bot se quedaría mudo justo cuando no sabe qué contestar")]
    TextoAusente(&'static str),
}

impl Guion {
    /// Comprueba que el guion sirve para atender.
    ///
    /// El aviso de privacidad **no se rellena con un defecto**. Es la directiva 20
    /// en el sitio donde más cara sale: un aviso de plantilla se ve igual de bien
    /// que uno correcto, el cliente asume que cumple, y la diferencia solo se
    /// nota el día que alguien reclama. Que el bot no arranque se nota en un
    /// minuto.
    pub fn validar(&self) -> Result<(), ErrorGuion> {
        if self.aviso_privacidad.trim().is_empty() {
            return Err(ErrorGuion::AvisoAusente);
        }
        if self.sin_respuesta.trim().is_empty() {
            return Err(ErrorGuion::TextoAusente("sin_respuesta"));
        }
        if self.escalando.trim().is_empty() {
            return Err(ErrorGuion::TextoAusente("escalando"));
        }
        if self.cancelado.trim().is_empty() {
            return Err(ErrorGuion::TextoAusente("cancelado"));
        }
        Ok(())
    }

    /// ¿Está pidiendo una persona?
    pub fn pide_persona(&self, mensaje: &str) -> bool {
        contiene_alguna(mensaje, &self.palabras_escalado)
    }

    /// ¿Está pidiendo salir del flujo? **Preguntar esto fuera de un flujo no
    /// tiene sentido**: quien llama es responsable de hacerlo solo dentro.
    pub fn pide_cancelar(&self, mensaje: &str) -> bool {
        contiene_alguna(mensaje, &self.palabras_cancelar)
    }
}

/// ¿Alguna de estas frases aparece completa en el mensaje?
///
/// Por palabras completas y no por subcadena: «asesor» está dentro de
/// «asesoría», y con subcadenas quien pregunta por asesorías acabaría en la cola.
fn contiene_alguna(mensaje: &str, frases: &[String]) -> bool {
    let texto = crate::texto::normalizar(mensaje);
    frases.iter().any(|p| {
        let aguja = crate::texto::normalizar(p);
        !aguja.is_empty() && crate::texto::contiene_palabras(&texto, &aguja)
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn guion() -> Guion {
        Guion {
            aviso_privacidad: "Sus datos los trata ACME S.A.S. …".into(),
            sin_respuesta: "No tengo esa respuesta.".into(),
            escalando: "Le paso con una persona.".into(),
            palabras_escalado: vec!["hablar con una persona".into(), "asesor".into()],
            palabras_cancelar: vec!["cancelar".into(), "ya no".into()],
            cancelado: "Listo, lo dejamos ahí.".into(),
            aviso_flujo: "(Puede escribir «cancelar» en cualquier momento.)".into(),
        }
    }

    #[test]
    fn un_guion_completo_vale() {
        assert_eq!(guion().validar(), Ok(()));
    }

    #[test]
    fn sin_aviso_de_privacidad_no_arranca() {
        let mut g = guion();
        g.aviso_privacidad = String::new();
        assert_eq!(g.validar(), Err(ErrorGuion::AvisoAusente));
    }

    #[test]
    fn un_aviso_de_solo_espacios_tampoco_vale() {
        // El caso rojo: `is_empty()` a secas lo daría por bueno y el cliente
        // recibiría un mensaje en blanco como aviso legal.
        let mut g = guion();
        g.aviso_privacidad = "   \n\t ".into();
        assert_eq!(g.validar(), Err(ErrorGuion::AvisoAusente));
    }

    #[test]
    fn sin_los_textos_de_salida_tampoco() {
        let mut g = guion();
        g.sin_respuesta = String::new();
        assert_eq!(g.validar(), Err(ErrorGuion::TextoAusente("sin_respuesta")));

        let mut g = guion();
        g.escalando = " ".into();
        assert_eq!(g.validar(), Err(ErrorGuion::TextoAusente("escalando")));
    }

    #[test]
    fn reconoce_la_peticion_de_una_persona_como_la_escriben() {
        let g = guion();
        assert!(g.pide_persona("quiero HABLAR CON UNA PERSONA por favor"));
        assert!(g.pide_persona("¿me pasas un asesor?"));
    }

    #[test]
    fn no_escala_por_una_palabra_dentro_de_otra() {
        let g = guion();
        // «asesor» está dentro de «asesoría»; si casara por subcadena, cualquiera
        // que preguntara por asesorías acabaría en la cola de un humano.
        assert!(!g.pide_persona("ofrecen asesorias contables"));
        assert!(!g.pide_persona("hola buenas"));
    }

    #[test]
    fn sin_palabras_configuradas_no_escala_por_esta_via() {
        let mut g = guion();
        g.palabras_escalado.clear();
        assert!(!g.pide_persona("hablar con una persona"));
    }

    #[test]
    fn sin_el_texto_de_cancelado_tampoco_vale() {
        // Va en `validar` y no en un defecto silencioso: si faltara, el bot se
        // quedaría MUDO justo al salirse de un pedido a medias.
        let mut g = guion();
        g.cancelado = "  ".into();
        assert_eq!(g.validar(), Err(ErrorGuion::TextoAusente("cancelado")));
    }

    #[test]
    fn reconoce_la_cancelacion_pero_no_por_subcadena() {
        let g = guion();
        assert!(g.pide_cancelar("cancelar por favor"));
        assert!(g.pide_cancelar("ya no, gracias"));
        // «cancelar» dentro de otra palabra no cuenta.
        assert!(!g.pide_cancelar("quiero cancelarlo todo"));
    }

    #[test]
    fn una_frase_vacia_en_la_lista_no_lo_dispara_todo() {
        // El caso rojo: `contiene_palabras(texto, "")` podría ser cierto siempre,
        // y entonces CUALQUIER mensaje cancelaría el flujo. Una fila en blanco en
        // el panel es de lo más fácil de dejar por accidente.
        let mut g = guion();
        g.palabras_cancelar = vec!["".into(), "  ".into()];
        assert!(!g.pide_cancelar("hola, quiero tres sillas"));
    }
}
