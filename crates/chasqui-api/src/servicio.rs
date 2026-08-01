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

use crate::canales::{AdaptadorCanal, meta_mensajeria::MetaMensajeria, whatsapp::WhatsApp};
use crate::config::Config;

/// El estado del servidor. Se clona barato (todo detrás de `Arc` o ya clonable).
#[derive(Clone)]
pub struct Servicio {
    pub pool: SqlitePool,
    pub boveda: Boveda,
    secreto_app: Arc<String>,
    token_verificacion: Arc<String>,
    canales: Arc<HashMap<Canal, Box<dyn AdaptadorCanal>>>,
    /// Cuántas horas dura una conversación. Sale de Meta; es configurable
    /// porque Meta la ha cambiado antes y cambiarla no debería exigir compilar.
    pub horas_ventana: i64,
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

        Servicio {
            pool,
            boveda: Boveda::nueva(&config.frase),
            secreto_app: Arc::new(config.secreto_app.clone()),
            token_verificacion: Arc::new(config.token_verificacion.clone()),
            canales: Arc::new(canales),
            horas_ventana: VENTANA_HORAS_META,
        }
    }

    /// El adaptador de un canal. Los tres están siempre montados: que una cuenta
    /// exista o no es cosa de la base, no del servidor.
    pub fn canal(&self, canal: Canal) -> &dyn AdaptadorCanal {
        self.canales
            .get(&canal)
            .map(AsRef::as_ref)
            .expect("los tres canales se montan en `nuevo`")
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
    async fn los_tres_canales_estan_montados() {
        // Que el canal sea una interfaz desde el primer día no sirve de nada si
        // el servidor solo monta uno: la ruta de Instagram daría 500.
        let config = Config {
            url_bd: "sqlite::memory:".into(),
            escucha: "127.0.0.1".parse().unwrap(),
            puerto: 8787,
            frase: "una frase larga de despliegue para las pruebas".into(),
            secreto_app: "secreto".into(),
            token_verificacion: "token".into(),
            graph: "https://graph.example/v21.0".into(),
        };
        // Un pool perezoso: no hace falta abrir la base para comprobar el montaje.
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let s = Servicio::nuevo(&config, pool);

        for canal in Canal::TODOS {
            assert_eq!(s.canal(canal).canal(), canal);
        }
    }
}
