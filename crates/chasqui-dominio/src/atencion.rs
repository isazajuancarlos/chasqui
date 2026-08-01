// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! La decisión: llega un mensaje, ¿qué se hace?
//!
//! Esta función es **pura**. No toca la base, no habla con Meta y no mira el
//! reloj: recibe el contexto ya reunido y devuelve la lista de acciones. Así la
//! regla que más caro sale equivocada —cuándo se manda el aviso legal, cuándo
//! calla el bot, cuándo se llama a una persona— se prueba entera sin levantar
//! nada, y las pruebas no pueden pasar «por la razón equivocada» porque no hay
//! infraestructura que las sostenga.
//!
//! ## La regla, en una frase
//!
//! Si no le toca al bot, calla; si la conversación es nueva, avisa; si piden una
//! persona, la llama; si la base de conocimiento responde, responde; y si no,
//! llama a una persona.
//!
//! El último tramo es el que casi nunca se implementa y el que decide si el
//! cliente apaga el bot a las dos semanas.

use crate::conocimiento::{Entrada, buscar};
use crate::conversacion::Estado;
use crate::guion::Guion;
use crate::mensaje::Clase;

/// Por qué se llamó a una persona. Se guarda para que el cliente pueda medirlo:
/// muchos `NoSabe` con el mismo texto son una entrada que falta en la base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motivo {
    /// Lo pidió quien escribe.
    Peticion,
    /// La base de conocimiento no tenía respuesta. Guarda el texto para poder
    /// convertirlo después en una entrada nueva.
    NoSabe(String),
    /// Llegó algo que el bot no sabe leer (una foto, un audio, una ubicación).
    NoSoportado(String),
}

/// Lo que hay que hacer, en orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accion {
    /// Mandar este texto por el mismo canal por el que entró.
    Enviar(String),
    /// Anotar que el aviso de privacidad ya salió en esta conversación.
    MarcarAvisoEnviado,
    /// Pasar la conversación a una persona.
    Escalar(Motivo),
}

/// Todo lo que hace falta para decidir, ya reunido por quien sí toca la base.
pub struct Contexto<'a> {
    pub clase: &'a Clase,
    pub estado: Estado,
    /// `true` cuando la conversación acaba de abrirse y el aviso no ha salido.
    pub debe_avisar: bool,
    pub guion: &'a Guion,
    pub conocimiento: &'a [Entrada],
}

/// Decide qué hacer con un mensaje entrante.
pub fn atender(ctx: &Contexto<'_>) -> Vec<Accion> {
    // 1. Si hay una persona en la conversación —o en camino—, el bot calla.
    //    Va PRIMERO y sin excepciones: una respuesta automática después de «le
    //    paso con alguien» es la que hace que el cliente desconfíe del bot entero.
    if !ctx.estado.atiende_el_bot() {
        return Vec::new();
    }

    let mut acciones = Vec::new();

    // 2. El aviso de privacidad va antes que cualquier respuesta. Si fuera
    //    después, el primer tratamiento de datos ya habría ocurrido sin avisar.
    if ctx.debe_avisar {
        acciones.push(Accion::Enviar(ctx.guion.aviso_privacidad.clone()));
        acciones.push(Accion::MarcarAvisoEnviado);
    }

    let texto = match ctx.clase.texto() {
        // 3. Lo que el bot no sabe leer no se responde a ciegas: va a una persona.
        None => {
            let tipo = match ctx.clase {
                Clase::NoSoportado(t) => t.clone(),
                // Inalcanzable hoy: solo `NoSoportado` devuelve `None`. Si mañana
                // otra clase lo hace, que el motivo lo diga en vez de mentir.
                otra => format!("{otra:?}"),
            };
            acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
            acciones.push(Accion::Escalar(Motivo::NoSoportado(tipo)));
            return acciones;
        }
        Some(t) => t,
    };

    // 4. Pedir una persona gana sobre la base de conocimiento. Al revés, quien
    //    escribe «necesito hablar con un asesor sobre el precio» recibiría la
    //    lista de precios y se quedaría sin asesor.
    if ctx.guion.pide_persona(texto) {
        acciones.push(Accion::Enviar(ctx.guion.escalando.clone()));
        acciones.push(Accion::Escalar(Motivo::Peticion));
        return acciones;
    }

    // 5. Lo que la base sabe responder.
    match buscar(texto, ctx.conocimiento) {
        Some(c) => acciones.push(Accion::Enviar(c.entrada.respuesta.clone())),
        // 6. Y lo que no se puede saber va a una persona. Aquí NO hay «responde
        //    lo más parecido»: ese es el atajo que convierte al bot en un oráculo
        //    que se equivoca con seguridad (directiva 23).
        None => {
            acciones.push(Accion::Enviar(ctx.guion.sin_respuesta.clone()));
            acciones.push(Accion::Escalar(Motivo::NoSabe(texto.to_string())));
        }
    }

    acciones
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn guion() -> Guion {
        Guion {
            aviso_privacidad: "AVISO".into(),
            sin_respuesta: "NO SÉ".into(),
            escalando: "TE PASO".into(),
            palabras_escalado: vec!["asesor".into(), "hablar con una persona".into()],
        }
    }

    fn base() -> Vec<Entrada> {
        vec![Entrada {
            id: 1,
            titulo: "Precios".into(),
            respuesta: "Desde $50.000.".into(),
            disparadores: vec!["precio".into(), "cuanto vale".into()],
        }]
    }

    fn ctx<'a>(
        clase: &'a Clase,
        estado: Estado,
        debe_avisar: bool,
        guion: &'a Guion,
        base: &'a [Entrada],
    ) -> Contexto<'a> {
        Contexto { clase, estado, debe_avisar, guion, conocimiento: base }
    }

    #[test]
    fn con_una_persona_en_la_conversacion_el_bot_no_dice_nada() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());
        for estado in [Estado::EsperandoHumano, Estado::ConHumano] {
            let acciones = atender(&ctx(&clase, estado, false, &g, &b));
            assert!(acciones.is_empty(), "{estado:?} debía callar, dijo {acciones:?}");
        }
    }

    #[test]
    fn el_silencio_gana_incluso_al_aviso_de_privacidad() {
        // Defensa en profundidad: hoy una conversación nueva siempre nace en
        // `Bot`, así que este caso no ocurre. Se fija para que, si mañana ocurre,
        // no sea el bot quien hable por encima de una persona.
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("hola".into());
        assert!(atender(&ctx(&clase, Estado::ConHumano, true, &g, &b)).is_empty());
    }

    #[test]
    fn el_aviso_va_primero_y_una_sola_vez() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale".into());

        let con_aviso = atender(&ctx(&clase, Estado::Bot, true, &g, &b));
        assert_eq!(
            con_aviso,
            vec![
                Accion::Enviar("AVISO".into()),
                Accion::MarcarAvisoEnviado,
                Accion::Enviar("Desde $50.000.".into()),
            ]
        );

        // Segundo mensaje de la misma conversación: ya no se avisa.
        let sin_aviso = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(sin_aviso, vec![Accion::Enviar("Desde $50.000.".into())]);
    }

    #[test]
    fn pedir_una_persona_gana_sobre_la_base_de_conocimiento() {
        let (g, b) = (guion(), base());
        // El texto casa con «precio» Y pide un asesor. Si ganara la base, quien
        // pide ayuda recibe una lista de precios y nadie lo atiende.
        let clase = Clase::Texto("necesito un asesor para saber el precio".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![Accion::Enviar("TE PASO".into()), Accion::Escalar(Motivo::Peticion)]
        );
    }

    #[test]
    fn lo_que_no_sabe_llama_a_una_persona_y_guarda_lo_que_le_preguntaron() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("¿hacen devoluciones?".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSabe("¿hacen devoluciones?".into())),
            ],
            "sin el texto guardado, el cliente no puede convertir los fallos en entradas nuevas"
        );
    }

    #[test]
    fn una_nota_de_voz_no_se_responde_como_si_fuera_texto_vacio() {
        let (g, b) = (guion(), base());
        let clase = Clase::NoSoportado("audio".into());
        let acciones = atender(&ctx(&clase, Estado::Bot, false, &g, &b));
        assert_eq!(
            acciones,
            vec![
                Accion::Enviar("NO SÉ".into()),
                Accion::Escalar(Motivo::NoSoportado("audio".into())),
            ]
        );
    }

    #[test]
    fn un_boton_se_atiende_por_su_titulo() {
        let (g, b) = (guion(), base());
        let clase = Clase::Interactivo { id: "m1".into(), titulo: "precio".into() };
        assert_eq!(
            atender(&ctx(&clase, Estado::Bot, false, &g, &b)),
            vec![Accion::Enviar("Desde $50.000.".into())]
        );
    }

    #[test]
    fn con_la_base_vacia_todo_va_a_una_persona_y_nada_se_queda_sin_respuesta() {
        // Simulación de la directiva 8 a nivel de regla: 120 mensajes distintos
        // contra una base vacía. Lo que se comprueba es que NINGUNO se queda sin
        // acción — un mensaje sin respuesta ni escalado es un cliente ignorado, y
        // es el fallo que no produce ningún error en el log.
        let g = guion();
        for i in 0..120 {
            let clase = Clase::Texto(format!("consulta número {i} sobre algo"));
            let acciones = atender(&ctx(&clase, Estado::Bot, i == 0, &g, &[]));
            assert!(
                acciones.iter().any(|a| matches!(a, Accion::Escalar(_))),
                "mensaje {i} se quedó sin escalar"
            );
            assert!(acciones.iter().any(|a| matches!(a, Accion::Enviar(_))));
        }
    }

    #[test]
    fn la_decision_es_determinista() {
        let (g, b) = (guion(), base());
        let clase = Clase::Texto("cuanto vale el servicio".into());
        let primera = atender(&ctx(&clase, Estado::Bot, true, &g, &b));
        for _ in 0..100 {
            assert_eq!(atender(&ctx(&clase, Estado::Bot, true, &g, &b)), primera);
        }
    }
}
