// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Qué es una conversación y cuándo empieza otra.
//!
//! No es una pregunta de estilo: de ella cuelgan el aviso de privacidad —que va
//! en el primer mensaje de **cada** conversación— y la factura, porque Meta cobra
//! por conversación iniciada y su ventana de servicio dura 24 horas.
//!
//! Se adopta la ventana de Meta como definición. Inventar una propia haría que el
//! bot dijera «conversación nueva» cuando Meta dice «la misma», y el cliente vería
//! dos números que no cuadran sin poder saber cuál es el bueno.

use chrono::{DateTime, Duration, Utc};

/// Las horas de la ventana de servicio de Meta. Es de ellos, no nuestra.
pub const VENTANA_HORAS_META: i64 = 24;

/// En qué estado está la conversación de cara al bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    /// El bot atiende.
    Bot,
    /// Se pidió una persona y todavía no ha entrado. El bot ya no responde: si
    /// siguiera, quien espera vería respuestas automáticas después de que le
    /// dijeran «te paso con alguien».
    EsperandoHumano,
    /// Hay una persona escribiendo. El bot calla.
    ConHumano,
}

impl Estado {
    /// La ranura que se guarda en la base.
    pub fn ranura(&self) -> &'static str {
        match self {
            Estado::Bot => "bot",
            Estado::EsperandoHumano => "esperando_humano",
            Estado::ConHumano => "con_humano",
        }
    }

    /// Lee un estado de la base. `None` si la fila trae algo que no es un estado
    /// — que se note, en vez de suponer `Bot` y poner al bot a hablar encima de
    /// una persona (directiva 20).
    pub fn desde_ranura(s: &str) -> Option<Estado> {
        match s {
            "bot" => Some(Estado::Bot),
            "esperando_humano" => Some(Estado::EsperandoHumano),
            "con_humano" => Some(Estado::ConHumano),
            _ => None,
        }
    }

    /// ¿Le toca contestar al bot?
    pub fn atiende_el_bot(&self) -> bool {
        matches!(self, Estado::Bot)
    }
}

/// ¿Se cerró la ventana entre el último mensaje y ahora?
///
/// `true` significa que el mensaje que llega abre una conversación nueva: se
/// vuelve a mandar el aviso de privacidad y Meta vuelve a cobrar.
pub fn ventana_cerrada(ultimo: DateTime<Utc>, ahora: DateTime<Utc>, horas: i64) -> bool {
    ahora - ultimo >= Duration::hours(horas)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn t(horas: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(0, 0).unwrap() + Duration::hours(horas)
    }

    #[test]
    fn dentro_de_la_ventana_sigue_siendo_la_misma_conversacion() {
        assert!(!ventana_cerrada(t(0), t(23), VENTANA_HORAS_META));
    }

    #[test]
    fn el_borde_exacto_ya_es_conversacion_nueva() {
        // A las 24 h justas Meta considera cerrada la ventana. Poner aquí un `>`
        // en vez de `>=` deja una hora en la que el aviso no se manda y Meta sí
        // cobra: el desacuerdo silencioso que este módulo existe para evitar.
        assert!(ventana_cerrada(t(0), t(24), VENTANA_HORAS_META));
        assert!(ventana_cerrada(t(0), t(25), VENTANA_HORAS_META));
    }

    #[test]
    fn un_reloj_que_va_para_atras_no_abre_conversacion() {
        // Puede pasar con un ajuste de hora del servidor. Lo que NO puede pasar
        // es que se lea como «hace mucho» y reinicie la conversación.
        assert!(!ventana_cerrada(t(10), t(9), VENTANA_HORAS_META));
    }

    #[test]
    fn las_ranuras_van_y_vuelven() {
        for e in [Estado::Bot, Estado::EsperandoHumano, Estado::ConHumano] {
            assert_eq!(Estado::desde_ranura(e.ranura()), Some(e));
        }
        assert_eq!(Estado::desde_ranura("humano"), None);
    }

    #[test]
    fn solo_el_estado_bot_contesta() {
        assert!(Estado::Bot.atiende_el_bot());
        assert!(!Estado::EsperandoHumano.atiende_el_bot());
        assert!(!Estado::ConHumano.atiende_el_bot());
    }
}
