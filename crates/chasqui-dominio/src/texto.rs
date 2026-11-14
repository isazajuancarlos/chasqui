// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Normalización de texto para comparar lo que escribe una persona con lo que el
//! cliente configuró.
//!
//! Quien escribe por WhatsApp escribe «cuanto vale», «Cuánto Vale?» y «CUÁNTO
//! VALE!!!». Las tres son la misma pregunta, y si el cliente tiene que enumerar
//! las variantes para que el bot conteste, el producto no sirve.
//!
//! Lo que se quita: mayúsculas, tildes, diéresis y signos de puntuación. Lo que
//! **no** se quita: la eñe. `año` y `ano` son palabras distintas en español, y
//! plegar la eñe a la ene es el error clásico de importar una normalización
//! pensada para el inglés.

/// Pliega un texto a su forma comparable: minúsculas, sin tildes, con los signos
/// convertidos en separadores y los espacios colapsados.
pub fn normalizar(entrada: &str) -> String {
    let mut salida = String::with_capacity(entrada.len());
    let mut espacio_pendiente = false;

    for c in entrada.chars().flat_map(|c| c.to_lowercase()) {
        let plegado = plegar(c);
        match plegado {
            Some(c) => {
                if espacio_pendiente && !salida.is_empty() {
                    salida.push(' ');
                }
                espacio_pendiente = false;
                salida.push(c);
            }
            // Todo lo que no es letra ni número separa palabras. Marcamos el
            // espacio en vez de escribirlo para no dejar espacios al final.
            None => espacio_pendiente = true,
        }
    }
    salida
}

/// Un carácter plegado, o `None` si es un separador.
fn plegar(c: char) -> Option<char> {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => Some('a'),
        'é' | 'è' | 'ë' | 'ê' => Some('e'),
        'í' | 'ì' | 'ï' | 'î' => Some('i'),
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => Some('o'),
        'ú' | 'ù' | 'ü' | 'û' => Some('u'),
        // La eñe se queda. Ver la nota del módulo.
        c if c.is_alphanumeric() => Some(c),
        _ => None,
    }
}

/// ¿Aparece `aguja` dentro de `pajar`, **como secuencia de palabras completas**?
///
/// Los dos textos tienen que venir ya normalizados. La comparación es por
/// palabras y no por subcadena a propósito: con subcadena, el disparador «no»
/// casaría dentro de «normalmente» y el cliente vería respuestas que no entiende.
pub fn contiene_palabras(pajar: &str, aguja: &str) -> bool {
    let aguja: Vec<&str> = aguja.split_whitespace().collect();
    if aguja.is_empty() {
        return false;
    }
    let pajar: Vec<&str> = pajar.split_whitespace().collect();
    pajar.windows(aguja.len()).any(|v| v == aguja.as_slice())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_variantes_de_la_misma_pregunta_se_pliegan_a_una() {
        let esperado = "cuanto vale";
        for variante in ["Cuánto vale", "CUÁNTO VALE!!!", "  cuanto   vale?  ", "¿Cuánto vale?"] {
            assert_eq!(normalizar(variante), esperado, "variante: {variante:?}");
        }
    }

    #[test]
    fn la_enye_no_se_pliega() {
        // El caso rojo: si esto diera «ano», el bot respondería lo del año.
        assert_eq!(normalizar("AÑO"), "año");
        assert_ne!(normalizar("año"), normalizar("ano"));
    }

    #[test]
    fn la_dieresis_si_se_pliega_porque_no_distingue_palabras() {
        assert_eq!(normalizar("pingüino"), "pinguino");
    }

    #[test]
    fn los_numeros_sobreviven() {
        assert_eq!(normalizar("Pedido #1234-B"), "pedido 1234 b");
    }

    #[test]
    fn contiene_exige_palabras_completas() {
        let pajar = normalizar("normalmente no atiendo");
        assert!(contiene_palabras(&pajar, &normalizar("no")));
        assert!(contiene_palabras(&pajar, &normalizar("no atiendo")));
        // El caso rojo de la comparación por subcadena: «nor» está dentro de
        // «normalmente», y aun así no es una palabra del texto.
        assert!(!contiene_palabras(&pajar, &normalizar("nor")));
        assert!(!contiene_palabras(&pajar, &normalizar("atiendo no")), "el orden importa");
    }

    #[test]
    fn una_aguja_vacia_no_casa_con_nada() {
        // Si casara, un disparador mal escrito («   ») respondería a TODO.
        assert!(!contiene_palabras("hola que tal", ""));
        assert!(!contiene_palabras("", ""));
    }
}
