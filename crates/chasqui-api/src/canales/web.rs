// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! El canal web: el chat del sitio del cliente y su aplicación móvil.
//!
//! # Por qué «enviar» aquí no envía nada
//!
//! Los tres canales de Meta responden con una llamada HTTP a la API Graph. Aquí
//! no hay a quién llamar: **el navegador es quien pregunta**. Y resulta que el
//! mensaje saliente ya se guarda en la tabla `mensaje` —lo hace `atender::enviar`
//! con el acuse de vuelta—, así que la entrega consiste en que el widget lea de
//! ahí. Este adaptador no necesita hacer nada.
//!
//! Eso no es un atajo: es lo que quita la causa (directiva 1). La alternativa era
//! una cola en memoria o un canal SSE por navegador conectado, y las dos añaden
//! estado que se pierde al reiniciar y que hay que sincronizar con lo que ya está
//! en la base. Con la base como única fuente, una respuesta escrita mientras el
//! visitante tenía el navegador cerrado le llega igual cuando vuelve.
//!
//! **Y un sondeo atraviesa cualquier proxy.** SSE se muere en cuanto un proxy
//! intermedio hace buffering —y Chasqui se instala en el servidor que tenga el
//! cliente, con el proxy que le haya puesto su administrador—: sería el fallo que
//! funciona en desarrollo y aparece muerto en casa del cliente.

use async_trait::async_trait;
use chasqui_dominio::{Canal, MensajeEntrante, MensajeSaliente};

use super::{AdaptadorCanal, ErrorCanal};

/// Cuánto texto admite un mensaje del canal web.
///
/// Hoy vale lo mismo que el de WhatsApp, y **eso es una coincidencia, no una
/// copia**. El de allá existe porque Meta RECHAZA el envío entero al pasarse: no
/// es negociable. Este es nuestro, y su razón es otra —una burbuja de chat de
/// veinte mil caracteres no se lee—, así que si mañana Meta cambia el suyo, este
/// no se mueve. Están escritos aparte para que no puedan arrastrarse.
const LIMITE: usize = 4096;

pub struct Web;

#[async_trait]
impl AdaptadorCanal for Web {
    fn canal(&self) -> Canal {
        Canal::Web
    }

    /// **Nunca se llama**: el canal web no llega por webhook.
    ///
    /// Devuelve error en vez de una lista vacía. Una lista vacía diría «llegó algo
    /// que no era un mensaje», que es un estado normal en los canales de Meta; con
    /// eso, montar por descuido `/webhook/web` daría 200 y silencio, y nadie
    /// sabría por qué no llega nada. El router además no crea esa ruta: esto es
    /// la segunda cerradura.
    fn entrantes(&self, _cuerpo: &serde_json::Value) -> Result<Vec<MensajeEntrante>, ErrorCanal> {
        Err(ErrorCanal::Forma {
            canal: Canal::Web,
            detalle: "el canal web no recibe webhooks: el navegador escribe en /web/mensaje"
                .into(),
        })
    }

    /// No hay a quién llamar. El mensaje ya queda registrado por el llamante, y
    /// el widget lo lee sondeando `/web/mensajes`.
    ///
    /// Devolver `Ok(None)` —y no un acuse inventado— es lo correcto: no hubo
    /// acuse de nadie, y un identificador falso en la columna del acuse haría
    /// creer que un tercero confirmó la entrega.
    async fn enviar(
        &self,
        _token: &str,
        _cuenta: &str,
        _mensaje: &MensajeSaliente,
    ) -> Result<Option<String>, ErrorCanal> {
        Ok(None)
    }

    fn limite_texto(&self) -> usize {
        LIMITE
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[tokio::test]
    async fn enviar_no_habla_con_nadie_y_no_inventa_un_acuse() {
        let w = Web;
        let m = MensajeSaliente::nuevo("visitante-1", "hola");
        assert_eq!(w.enviar("", "sitio", &m).await.unwrap(), None);
    }

    #[test]
    fn un_webhook_dirigido_al_canal_web_se_denuncia_en_vez_de_pasar_por_vacio() {
        let r = Web.entrantes(&serde_json::json!({"object": "whatsapp_business_account"}));
        assert!(r.is_err(), "un webhook al canal web pasó por «no era un mensaje»");
    }

    #[test]
    fn una_respuesta_larga_se_parte_sin_perder_nada() {
        // Lo que de verdad importa del límite: que partir no sea truncar. La
        // mitad de una respuesta parece una respuesta.
        let largo = "palabra ".repeat(2000);
        let partes = super::super::trozos(&largo, Web.limite_texto());
        assert!(partes.len() > 1);
        assert_eq!(partes.join(" ").split_whitespace().count(), 2000);
    }
}
