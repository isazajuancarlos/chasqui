// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! La base de conocimiento: lo que el cliente escribe una vez y el bot contesta
//! mil veces.
//!
//! **La regla, en una frase**: responde la entrada activa cuyo disparador más
//! largo aparezca completo en el mensaje; si ninguna aparece, no se sabe.
//!
//! No hay «en cualquier otro caso» (directiva 23). Lo que no casa no se responde
//! con la entrada más parecida: se manda a una persona. Un bot que contesta el
//! horario a quien pregunta por una devolución hace más daño que uno que dice
//! «te paso con alguien» — el primero parece que funciona.
//!
//! El desempate por longitud es lo que deja convivir «precio» con «precio del
//! envío»: gana el disparador más específico, no el que esté primero en la tabla.

use crate::texto::{contiene_palabras, normalizar};

/// Una entrada de la base de conocimiento, tal como la configura el cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrada {
    pub id: i64,
    /// Para que el cliente la reconozca en el panel. No participa en la búsqueda.
    pub titulo: String,
    pub respuesta: String,
    /// Las frases que la disparan. Se comparan normalizadas y por palabras
    /// completas.
    pub disparadores: Vec<String>,
}

/// Lo que casó: la entrada y el disparador que la trajo.
///
/// El disparador viaja de vuelta porque es lo que se le enseña al cliente cuando
/// pregunta «¿por qué contestó esto?». Sin él, la respuesta del bot es un oráculo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coincidencia<'a> {
    pub entrada: &'a Entrada,
    pub disparador: String,
}

/// Busca la entrada que responde `mensaje`. `None` si no se puede saber.
pub fn buscar<'a>(mensaje: &str, entradas: &'a [Entrada]) -> Option<Coincidencia<'a>> {
    let texto = normalizar(mensaje);
    if texto.is_empty() {
        return None;
    }

    let mut mejor: Option<Coincidencia<'a>> = None;
    for entrada in entradas {
        for disparador in &entrada.disparadores {
            let aguja = normalizar(disparador);
            if !contiene_palabras(&texto, &aguja) {
                continue;
            }
            // Más largo gana. Con longitudes iguales manda el que ya estaba: el
            // orden de la tabla desempata, y así el resultado es determinista
            // aunque el cliente configure dos disparadores iguales.
            let mejora = match &mejor {
                None => true,
                Some(actual) => aguja.len() > actual.disparador.len(),
            };
            if mejora {
                mejor = Some(Coincidencia { entrada, disparador: aguja });
            }
        }
    }
    mejor
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn entrada(id: i64, respuesta: &str, disparadores: &[&str]) -> Entrada {
        Entrada {
            id,
            titulo: format!("entrada {id}"),
            respuesta: respuesta.into(),
            disparadores: disparadores.iter().map(|d| d.to_string()).collect(),
        }
    }

    fn base() -> Vec<Entrada> {
        vec![
            entrada(1, "De 8 a 6.", &["horario", "a que hora abren"]),
            entrada(2, "Desde $50.000.", &["precio", "cuanto vale"]),
            entrada(3, "El envío cuesta $12.000.", &["precio del envio", "cuanto vale el envio"]),
        ]
    }

    #[test]
    fn responde_lo_que_casa() {
        let base = base();
        let c = buscar("¿Cuál es el HORARIO?", &base).expect("debía casar");
        assert_eq!(c.entrada.id, 1);
        assert_eq!(c.disparador, "horario");
    }

    #[test]
    fn gana_el_disparador_mas_especifico_no_el_primero_de_la_tabla() {
        // «precio» (entrada 2) casa antes en el recorrido, pero «precio del
        // envio» (entrada 3) es más largo. Sin este desempate, quien pregunta por
        // el envío recibe el precio del producto: un falso positivo que el
        // cliente ve como «el bot miente».
        let base = base();
        let c = buscar("hola, cuál es el precio del envío?", &base).expect("debía casar");
        assert_eq!(c.entrada.id, 3);
    }

    #[test]
    fn lo_que_no_casa_no_se_responde_con_lo_mas_parecido() {
        // El corazón de la directiva 23: sin «en cualquier otro caso».
        let base = base();
        assert!(buscar("quiero devolver un producto defectuoso", &base).is_none());
        assert!(buscar("", &base).is_none());
        assert!(buscar("   ¿?  ", &base).is_none(), "solo signos es texto vacío");
    }

    #[test]
    fn una_base_vacia_no_responde_nada() {
        assert!(buscar("horario", &[]).is_none());
    }

    #[test]
    fn un_disparador_no_casa_dentro_de_otra_palabra() {
        // Caso rojo de la comparación por subcadena.
        let base = vec![entrada(1, "…", &["no"])];
        assert!(buscar("normalmente pido a domicilio", &base).is_none());
        assert!(buscar("no gracias", &base).is_some());
    }

    #[test]
    fn el_resultado_no_depende_del_orden_en_que_se_escriban_los_disparadores() {
        let al_derecho = vec![entrada(1, "…", &["precio", "precio del envio"])];
        let al_reves = vec![entrada(1, "…", &["precio del envio", "precio"])];
        let m = "cual es el precio del envio";
        assert_eq!(
            buscar(m, &al_derecho).unwrap().disparador,
            buscar(m, &al_reves).unwrap().disparador
        );
    }
}
