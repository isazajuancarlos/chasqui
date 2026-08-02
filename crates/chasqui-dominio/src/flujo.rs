// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Flujos de varios pasos: recoger datos seguidos y armar un pedido.
//!
//! **Lo piden tres de las cinco peticiones de Workana** —«lógica para la gestión
//! de flujos de conversación», «flujos de conversación para guiar a los usuarios
//! hacia soluciones», «comprensión de flujos de atención»—, así que es lo segundo
//! más pedido después de las preguntas frecuentes. La toma de pedidos es una de
//! esas tres: un flujo cuyo final es un pedido armado.
//!
//! Como todo lo demás, se resuelve **configurando y no programando**: el cliente
//! declara los pasos y nosotros no escribimos código por cliente.
//!
//! # Un flujo CAPTURA la conversación, y hay que decirlo
//!
//! Mientras uno está en curso, un paso de texto libre se queda con lo que llegue:
//! no hay forma de distinguir «Calle 5 #3-20» de un cambio de tema, y adivinarlo
//! sería inventar. Por eso el flujo tiene tres salidas, y las tres van ANTES de
//! consumir el mensaje:
//!
//! 1. **pedir una persona** — gana siempre, en un flujo y fuera de él;
//! 2. **cancelar** — la palabra la configura el cliente y solo significa algo
//!    DENTRO de un flujo: fuera, «cancelar» es una palabra más de una frase, y
//!    tratarla como orden rompería a quien pregunta «¿puedo cancelar un pedido?»;
//! 3. **no saber contestar un paso tipado** — se pregunta una vez y, si sigue sin
//!    llegar, va a una persona con lo recogido hasta ahí.
//!
//! Y por eso existe `guion.aviso_flujo`: el texto que se añade a la primera
//! pregunta para que quien escribe sepa que puede salir. Un flujo sin salida
//! anunciada es el bot que se apaga a las dos semanas.
//!
//! # Lo que NO se pierde
//!
//! Un flujo interrumpido **no se tira**: lo recogido viaja al escalado. Quien
//! atiende ve «Producto: 3 sillas / Cantidad: 3» y termina el pedido en un
//! mensaje, en vez de volver a preguntarlo todo. Tirarlo sería castigar al
//! cliente por haber empezado.

use crate::consulta::TipoDato;
use crate::texto::{contiene_palabras, normalizar};

/// Un paso: una pregunta y el dato que recoge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paso {
    /// Con qué nombre queda el dato. Es lo que se usa en la confirmación y lo que
    /// verá quien reciba el pedido: `producto`, `cantidad`, `direccion`.
    pub clave: String,
    pub pregunta: String,
    /// Qué clase de dato se espera. `None` es texto libre: vale lo que llegue.
    ///
    /// Tiparlo no es cosmética — es lo que hace que «tres» no pase por una
    /// cantidad y que el pedido no llegue al negocio con un campo inservible.
    pub tipo_dato: Option<TipoDato>,
}

/// Un flujo configurado por el cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flujo {
    pub id: i64,
    pub titulo: String,
    /// Las frases que lo arrancan. Mismo criterio que todo lo demás.
    pub disparadores: Vec<String>,
    pub pasos: Vec<Paso>,
    /// Lo que se dice al terminar, con `{clave}` de cada paso.
    pub confirmacion: String,
}

/// Un flujo a medias, tal como lo lleva la conversación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnCurso {
    pub flujo_id: i64,
    /// Lo recogido, en el orden de los pasos. Su longitud dice en qué paso va.
    pub recogido: Vec<(String, String)>,
    /// Si ya se volvió a preguntar por el paso actual.
    ///
    /// Es la misma regla de las consultas y por el mismo motivo: preguntar dos
    /// veces lo mismo es un bucle, y un bot que repite es peor que uno que no
    /// sabe. Se pregunta UNA vez más y luego va a una persona.
    pub repreguntado: bool,
}

/// Por qué un flujo no sirve. Se comprueba al cargarlo, no al usarlo: un flujo
/// mal configurado que revienta a mitad de conversación deja al cliente del
/// cliente colgado, y el error lo ve quien no puede arreglarlo.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorFlujo {
    #[error("el flujo «{0}» no tiene pasos: no recogería nada")]
    SinPasos(String),

    #[error("el flujo «{0}» no tiene disparadores: nada lo arrancaría nunca")]
    SinDisparadores(String),

    #[error("el flujo «{flujo}» tiene un paso sin {que}")]
    PasoIncompleto { flujo: String, que: &'static str },

    #[error(
        "el flujo «{flujo}» repite la clave «{clave}». Dos pasos con el mismo nombre se pisan: \
         el segundo borraría lo que contestó el primero."
    )]
    ClaveRepetida { flujo: String, clave: String },

    #[error(
        "la confirmación del flujo «{flujo}» usa «{{{clave}}}», que no es ninguno de sus pasos. \
         Se rechaza al cargarlo y no al enviarlo: si no, el cliente recibiría la frase con un \
         hueco justo al confirmar su pedido."
    )]
    ClaveDesconocida { flujo: String, clave: String },
}

impl Flujo {
    /// Comprueba que el flujo sirve antes de guardarlo.
    pub fn validar(&self) -> Result<(), ErrorFlujo> {
        if self.disparadores.iter().all(|d| d.trim().is_empty()) {
            return Err(ErrorFlujo::SinDisparadores(self.titulo.clone()));
        }
        if self.pasos.is_empty() {
            return Err(ErrorFlujo::SinPasos(self.titulo.clone()));
        }

        let mut vistas: Vec<&str> = Vec::new();
        for paso in &self.pasos {
            if paso.clave.trim().is_empty() {
                return Err(ErrorFlujo::PasoIncompleto {
                    flujo: self.titulo.clone(),
                    que: "clave",
                });
            }
            if paso.pregunta.trim().is_empty() {
                return Err(ErrorFlujo::PasoIncompleto {
                    flujo: self.titulo.clone(),
                    que: "pregunta",
                });
            }
            if vistas.contains(&paso.clave.as_str()) {
                return Err(ErrorFlujo::ClaveRepetida {
                    flujo: self.titulo.clone(),
                    clave: paso.clave.clone(),
                });
            }
            vistas.push(&paso.clave);
        }

        for clave in claves_de(&self.confirmacion) {
            if !vistas.contains(&clave.as_str()) {
                return Err(ErrorFlujo::ClaveDesconocida {
                    flujo: self.titulo.clone(),
                    clave,
                });
            }
        }
        Ok(())
    }

    /// El paso que toca cuando ya se recogieron `cuantos`.
    pub fn paso(&self, cuantos: usize) -> Option<&Paso> {
        self.pasos.get(cuantos)
    }
}

/// Las `{claves}` que aparecen en una plantilla.
fn claves_de(plantilla: &str) -> Vec<String> {
    let mut salida = Vec::new();
    let mut resto = plantilla;
    while let Some(abre) = resto.find('{') {
        let tras = &resto[abre + 1..];
        let Some(cierra) = tras.find('}') else { break };
        salida.push(tras[..cierra].to_string());
        resto = &tras[cierra + 1..];
    }
    salida
}

/// Busca el flujo que arranca con este mensaje.
///
/// Mismo criterio que la base de conocimiento y las consultas: **gana el
/// disparador más largo**. Que las tres tablas compitan con una sola regla es lo
/// que permite que el cliente prediga por qué el bot hizo lo que hizo sin saber
/// en qué tabla está cada cosa.
pub fn buscar<'a>(mensaje: &str, flujos: &'a [Flujo]) -> Option<(&'a Flujo, String)> {
    let texto = normalizar(mensaje);
    if texto.is_empty() {
        return None;
    }

    let mut mejor: Option<(&'a Flujo, String)> = None;
    for flujo in flujos {
        for disparador in &flujo.disparadores {
            let aguja = normalizar(disparador);
            if aguja.is_empty() || !contiene_palabras(&texto, &aguja) {
                continue;
            }
            let mejora = match &mejor {
                None => true,
                Some((_, actual)) => aguja.len() > actual.len(),
            };
            if mejora {
                mejor = Some((flujo, aguja));
            }
        }
    }
    mejor
}

/// Rellena la confirmación con lo recogido.
///
/// No puede fallar por un campo ausente porque [`Flujo::validar`] ya rechazó las
/// claves que no son pasos, y un flujo solo se confirma cuando TODOS los pasos se
/// recogieron. Quitar la causa en vez de manejar el síntoma (directiva 1).
pub fn confirmar(flujo: &Flujo, recogido: &[(String, String)]) -> String {
    let mut salida = String::with_capacity(flujo.confirmacion.len());
    let mut resto = flujo.confirmacion.as_str();
    while let Some(abre) = resto.find('{') {
        salida.push_str(&resto[..abre]);
        let tras = &resto[abre + 1..];
        let Some(cierra) = tras.find('}') else {
            // Una llave sin cerrar es texto: el cliente escribió una llave.
            salida.push_str(&resto[abre..]);
            return salida;
        };
        let clave = &tras[..cierra];
        match recogido.iter().find(|(k, _)| k == clave) {
            Some((_, v)) => salida.push_str(v),
            // Inalcanzable con un flujo validado y completo. Se deja la llave tal
            // cual en vez de un hueco: un texto raro se ve; un hueco, no.
            None => salida.push_str(&resto[abre..=abre + cierra + 1]),
        }
        resto = &tras[cierra + 1..];
    }
    salida.push_str(resto);
    salida
}

/// Lo recogido, en un texto para quien lo va a atender.
///
/// Va al escalado, que es la única superficie donde una persona ve el pedido. Un
/// pedido que solo existe en la base es un pedido que nadie despacha.
pub fn resumen(flujo: &Flujo, recogido: &[(String, String)]) -> String {
    let mut salida = flujo.titulo.clone();
    for (clave, valor) in recogido {
        salida.push_str(&format!("\n{clave}: {valor}"));
    }
    if recogido.len() < flujo.pasos.len() {
        let faltan: Vec<&str> = flujo.pasos[recogido.len()..]
            .iter()
            .map(|p| p.clave.as_str())
            .collect();
        salida.push_str(&format!("\n(quedó a medias; falta: {})", faltan.join(", ")));
    }
    salida
}

#[cfg(test)]
mod pruebas {
    use super::*;

    pub(crate) fn pedido() -> Flujo {
        Flujo {
            id: 3,
            titulo: "Toma de pedido".into(),
            disparadores: vec!["quiero hacer un pedido".into(), "hacer pedido".into()],
            pasos: vec![
                Paso {
                    clave: "producto".into(),
                    pregunta: "¿Qué producto desea?".into(),
                    tipo_dato: None,
                },
                Paso {
                    clave: "cantidad".into(),
                    pregunta: "¿Cuántas unidades?".into(),
                    tipo_dato: Some(TipoDato::Numero),
                },
                Paso {
                    clave: "direccion".into(),
                    pregunta: "¿A qué dirección lo enviamos?".into(),
                    tipo_dato: None,
                },
            ],
            confirmacion: "Anotado: {cantidad} de {producto} para {direccion}.".into(),
        }
    }

    #[test]
    fn un_flujo_bien_configurado_vale() {
        assert_eq!(pedido().validar(), Ok(()));
    }

    #[test]
    fn una_confirmacion_que_nombra_un_paso_inexistente_se_rechaza_al_cargar() {
        // El caso que importa: si esto pasara la validación, el cliente recibiría
        // «Anotado: 3 de sillas para {telefono}.» justo al confirmar su pedido.
        let mut f = pedido();
        f.confirmacion = "Anotado para {telefono}.".into();
        assert_eq!(
            f.validar(),
            Err(ErrorFlujo::ClaveDesconocida {
                flujo: "Toma de pedido".into(),
                clave: "telefono".into()
            })
        );
    }

    #[test]
    fn dos_pasos_con_la_misma_clave_se_rechazan() {
        let mut f = pedido();
        f.pasos[1].clave = "producto".into();
        assert!(matches!(f.validar(), Err(ErrorFlujo::ClaveRepetida { .. })));
    }

    #[test]
    fn un_flujo_sin_pasos_o_sin_disparadores_no_sirve() {
        let mut f = pedido();
        f.pasos.clear();
        assert!(matches!(f.validar(), Err(ErrorFlujo::SinPasos(_))));

        let mut f = pedido();
        f.disparadores = vec!["  ".into()];
        assert!(matches!(f.validar(), Err(ErrorFlujo::SinDisparadores(_))));
    }

    #[test]
    fn un_paso_sin_pregunta_no_sirve() {
        // Recogería un dato sin haberlo pedido: el bot se quedaría callado
        // esperando una respuesta que nadie sabe que tiene que dar.
        let mut f = pedido();
        f.pasos[1].pregunta = " ".into();
        assert!(matches!(f.validar(), Err(ErrorFlujo::PasoIncompleto { que: "pregunta", .. })));
    }

    #[test]
    fn gana_el_disparador_mas_largo_tambien_entre_flujos() {
        let corto = Flujo { id: 1, disparadores: vec!["pedido".into()], ..pedido() };
        let largo = Flujo {
            id: 2,
            disparadores: vec!["pedido internacional".into()],
            ..pedido()
        };
        let flujos = vec![corto, largo];
        let (f, _) = buscar("quiero un pedido internacional", &flujos).unwrap();
        assert_eq!(f.id, 2);
    }

    #[test]
    fn no_arranca_por_una_palabra_dentro_de_otra() {
        let flujos = vec![pedido()];
        assert!(buscar("¿tienen pedidos pendientes?", &flujos).is_none());
        assert!(buscar("", &flujos).is_none());
    }

    #[test]
    fn la_confirmacion_se_rellena_con_lo_recogido() {
        let f = pedido();
        let recogido = vec![
            ("producto".to_string(), "sillas".to_string()),
            ("cantidad".to_string(), "3".to_string()),
            ("direccion".to_string(), "Calle 5 #3-20".to_string()),
        ];
        assert_eq!(
            confirmar(&f, &recogido),
            "Anotado: 3 de sillas para Calle 5 #3-20."
        );
    }

    #[test]
    fn el_resumen_dice_lo_que_falta_cuando_quedo_a_medias() {
        // Es lo que ve quien atiende. Sin la lista de lo que falta, tendría que
        // abrir el flujo en el panel para saber qué preguntar.
        let f = pedido();
        let recogido = vec![("producto".to_string(), "sillas".to_string())];
        let r = resumen(&f, &recogido);
        assert!(r.contains("producto: sillas"));
        assert!(r.contains("falta: cantidad, direccion"), "{r}");
    }

    #[test]
    fn el_resumen_completo_no_habla_de_lo_que_falta() {
        let f = pedido();
        let recogido = vec![
            ("producto".to_string(), "sillas".to_string()),
            ("cantidad".to_string(), "3".to_string()),
            ("direccion".to_string(), "Calle 5".to_string()),
        ];
        assert!(!resumen(&f, &recogido).contains("falta"));
    }

    #[test]
    fn una_llave_sin_cerrar_en_la_confirmacion_es_texto_y_no_rompe_nada() {
        let mut f = pedido();
        f.confirmacion = "Listo {producto} y un 50% {de descuento".into();
        f.validar().unwrap();
        let recogido = vec![("producto".to_string(), "sillas".to_string())];
        assert_eq!(confirmar(&f, &recogido), "Listo sillas y un 50% {de descuento");
    }
}
