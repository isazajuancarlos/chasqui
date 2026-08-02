// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
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
