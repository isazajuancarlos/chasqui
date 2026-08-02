// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Lo que comparten todos los manejadores: la base, la bóveda de credenciales,
//! los secretos de Meta y los adaptadores de canal.

use std::collections::HashMap;
use std::sync::Arc;

use chasqui_datos::Boveda;
use chasqui_dominio::Canal;
use chasqui_dominio::conversacion::VENTANA_HORAS_META;
use sqlx::SqlitePool;

use crate::canales::{AdaptadorCanal, meta_mensajeria::MetaMensajeria, web::Web, whatsapp::WhatsApp};
use crate::config::Config;

/// El estado del servidor. Se clona barato (todo detrás de `Arc` o ya clonable).
#[derive(Clone)]
pub struct Servicio {
    pub pool: SqlitePool,
    pub boveda: Boveda,
    secreto_app: Arc<String>,
    token_verificacion: Arc<String>,
    canales: Arc<HashMap<Canal, Box<dyn AdaptadorCanal>>>,
    /// El cliente HTTP con el que se le pregunta al sistema del cliente. Va
    /// aparte del de Meta: la paciencia con la API de un negocio y la paciencia
    /// con Meta no tienen por qué ser la misma, y mezclarlas ata dos decisiones
    /// que se revisan por motivos distintos.
    pub hacia_el_cliente: reqwest::Client,
    /// Cuántas horas dura una conversación. Sale de Meta; es configurable
    /// porque Meta la ha cambiado antes y cambiarla no debería exigir compilar.
    pub horas_ventana: i64,
    /// Con qué se firman las sesiones del panel. Derivado de la frase; ver
    /// [`crate::sesion::secreto_de`].
    secreto_sesion: Arc<Vec<u8>>,
    /// Si la cookie de sesión va sin `Secure`. Solo para probar en local.
    pub sin_tls: bool,
    /// Cuánto dura una conversación del canal web.
    ///
    /// **No son las 24 horas de Meta.** Aquella es una regla de FACTURACIÓN de
    /// Meta, no una propiedad de las conversaciones: aplicarla a un chat de una
    /// página web sería cerrarle la conversación a alguien por un motivo que en
    /// su canal no existe. Una semana: lo bastante para que quien vuelva al día
    /// siguiente encuentre el hilo, y lo bastante corto para que el historial no
    /// sea eterno.
    pub horas_ventana_web: i64,
    /// El freno de conversaciones nuevas del canal web, por IP y en memoria.
    pub freno_web: crate::web::Freno,
}

/// El cliente HTTP para las consultas al sistema del cliente.
///
/// **Sin redirecciones.** Una API que contesta 302 hacia otro sitio se llevaría
/// con ella la cabecera `Authorization` del cliente — sus credenciales, a un
/// servidor que él no configuró. Que falle y se vea.
fn cliente_de_consultas() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("el cliente HTTP se construye con la configuración por defecto")
}

impl Servicio {
    pub fn nuevo(config: &Config, pool: SqlitePool) -> Servicio {
        // Un solo cliente HTTP para todo: reutiliza conexiones. Uno por envío
        // rehace el saludo TLS con Meta en cada mensaje.
        let cliente = reqwest::Client::builder()
            // Sin límite, un Meta que no contesta deja colgado el manejador del
            // webhook y Meta reintenta por encima. Diez segundos es más de lo
            // que tarda un envío bueno y menos de lo que Meta espera.
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("el cliente HTTP se construye con la configuración por defecto");

        let mut canales: HashMap<Canal, Box<dyn AdaptadorCanal>> = HashMap::new();
        canales.insert(
            Canal::WhatsApp,
            Box::new(WhatsApp::nuevo(cliente.clone(), &config.graph)),
        );
        canales.insert(
            Canal::Messenger,
            Box::new(MetaMensajeria::nuevo(Canal::Messenger, cliente.clone(), &config.graph)),
        );
        canales.insert(
            Canal::Instagram,
            Box::new(MetaMensajeria::nuevo(Canal::Instagram, cliente, &config.graph)),
        );
        // El canal web no habla con Meta y no necesita cliente HTTP ninguno: su
        // «envío» es que el mensaje quede en la base, donde el widget lo lee.
        canales.insert(Canal::Web, Box::new(Web));

        Servicio {
            pool,
            hacia_el_cliente: cliente_de_consultas(),
            boveda: Boveda::nueva(&config.frase),
            secreto_app: Arc::new(config.secreto_app.clone()),
            token_verificacion: Arc::new(config.token_verificacion.clone()),
            canales: Arc::new(canales),
            horas_ventana: VENTANA_HORAS_META,
            secreto_sesion: Arc::new(crate::sesion::secreto_de(&config.frase)),
            sin_tls: config.sin_tls,
            horas_ventana_web: 24 * 7,
            freno_web: crate::web::Freno::default(),
        }
    }

    /// El secreto con el que se firman las sesiones del panel.
    pub fn secreto_sesion(&self) -> &[u8] {
        &self.secreto_sesion
    }

    /// El adaptador de un canal. Los cuatro están siempre montados: que una
    /// cuenta exista o no es cosa de la base, no del servidor.
    pub fn canal(&self, canal: Canal) -> &dyn AdaptadorCanal {
        self.canales
            .get(&canal)
            .map(AsRef::as_ref)
            .expect("los cuatro canales se montan en `nuevo`")
    }

    /// El secreto con el que se comprueba la firma de un webhook.
    ///
    /// Va por método y no por campo público para que no acabe en un `{:?}` ni en
    /// una respuesta JSON por descuido.
    pub fn secreto_app(&self) -> &str {
        &self.secreto_app
    }

    pub fn token_verificacion(&self) -> &str {
        &self.token_verificacion
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    // `connect_lazy` de SQLx exige un contexto de tokio aunque no abra nada.
    #[tokio::test]
    async fn todos_los_canales_estan_montados() {
        // Que el canal sea una interfaz desde el primer día no sirve de nada si
        // el servidor solo monta uno: la ruta de Instagram daría 500. Recorre
        // `Canal::TODOS`, así que añadir un canal y no montarlo se cae aquí.
        let config = Config {
            url_bd: "sqlite::memory:".into(),
            escucha: "127.0.0.1".parse().unwrap(),
            puerto: 8787,
            frase: "una frase larga de despliegue para las pruebas".into(),
            secreto_app: "secreto".into(),
            token_verificacion: "token".into(),
            graph: "https://graph.example/v21.0".into(),
            sin_tls: true,
        };
        // Un pool perezoso: no hace falta abrir la base para comprobar el montaje.
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let s = Servicio::nuevo(&config, pool);

        for canal in Canal::TODOS {
            assert_eq!(s.canal(canal).canal(), canal);
        }
    }
}
