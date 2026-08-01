// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Cuánto tiempo se guarda una conversación.
//!
//! La Ley 1581 de 2012 no da un número: obliga a que el dato no se conserve más
//! de lo necesario para la finalidad. O sea que **el plazo lo decide el cliente**,
//! que es el responsable del tratamiento, y nosotros no podemos decidirlo por él.
//!
//! De ahí la regla de este módulo: **sin plazo configurado no se borra nada, y
//! se dice a gritos**. Las dos mitades importan. Borrar por defecto destruiría el
//! historial de un negocio que nunca lo pidió; callar convertiría una promesa de
//! la propuesta en una casilla que nadie marcó nunca.

use chrono::{DateTime, Duration, Utc};

/// El plazo más corto que se admite.
///
/// Un día. Menos no es un plazo de retención: es borrar la conversación mientras
/// ocurre, y quien lo configurara así perdería el historial de una queja antes de
/// poder atenderla.
pub const MINIMO_DIAS: i64 = 1;

/// El plazo más largo que se admite, en días. Diez años.
///
/// No es una restricción de privacidad sino un filtro de errata: quien escribe
/// «36500» queriendo «365» configura un plazo que no vence nunca, y el bot
/// pasaría por cumplidor sin borrar jamás. Diez años es más que cualquier
/// finalidad comercial y menos que un cero de más.
pub const MAXIMO_DIAS: i64 = 3650;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorPlazo {
    #[error(
        "«{0}» no es un número de días. El plazo de retención se escribe en días: «90», no \
         «3 meses»."
    )]
    NoEsNumero(String),

    #[error(
        "un plazo de {0} días no es un plazo de retención. El mínimo es {MINIMO_DIAS} día: por \
         debajo, la conversación se borra mientras todavía se está atendiendo."
    )]
    DemasiadoCorto(i64),

    #[error(
        "un plazo de {0} días ({1} años) no vence en la práctica. ¿Sobran ceros? El máximo es \
         {MAXIMO_DIAS} días."
    )]
    DemasiadoLargo(i64, i64),
}

/// Lee el plazo configurado por el cliente.
pub fn plazo(valor: &str) -> Result<i64, ErrorPlazo> {
    let dias: i64 =
        valor.trim().parse().map_err(|_| ErrorPlazo::NoEsNumero(valor.trim().to_string()))?;

    if dias < MINIMO_DIAS {
        return Err(ErrorPlazo::DemasiadoCorto(dias));
    }
    if dias > MAXIMO_DIAS {
        return Err(ErrorPlazo::DemasiadoLargo(dias, dias / 365));
    }
    Ok(dias)
}

/// La fecha por debajo de la cual todo se borra.
///
/// Lo que quede exactamente en el borde **se conserva**: el plazo dice «no más de
/// noventa días», y una conversación de hace noventa días justos todavía está
/// dentro. Redondear hacia el borrado es perder un día de datos del cliente por
/// una decisión que nadie tomó.
pub fn corte(ahora: DateTime<Utc>, dias: i64) -> DateTime<Utc> {
    ahora - Duration::days(dias)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn un_plazo_normal_se_lee() {
        assert_eq!(plazo("90"), Ok(90));
        assert_eq!(plazo(" 365 "), Ok(365));
    }

    #[test]
    fn lo_que_no_es_un_numero_se_denuncia_con_lo_que_hay_que_escribir() {
        // El cliente escribe «3 meses» en el panel. El error tiene que decirle
        // qué poner, no «valor inválido».
        assert_eq!(plazo("3 meses"), Err(ErrorPlazo::NoEsNumero("3 meses".into())));
        assert_eq!(plazo(""), Err(ErrorPlazo::NoEsNumero("".into())));
    }

    #[test]
    fn cero_dias_no_es_un_plazo() {
        assert_eq!(plazo("0"), Err(ErrorPlazo::DemasiadoCorto(0)));
        assert_eq!(plazo("-5"), Err(ErrorPlazo::DemasiadoCorto(-5)));
    }

    #[test]
    fn un_cero_de_mas_se_caza_antes_de_guardarlo() {
        // La errata que produce un sistema que PARECE cumplir: 36 500 días son
        // cien años, o sea nunca, y nadie lo notaría jamás.
        assert!(matches!(plazo("36500"), Err(ErrorPlazo::DemasiadoLargo(36500, 100))));
    }

    #[test]
    fn los_bordes_del_rango_valen() {
        // La regla se prueba contra la operación normal, no contra el atacante
        // (directiva 21): un día y diez años son inusuales, no imposibles.
        assert_eq!(plazo(&MINIMO_DIAS.to_string()), Ok(MINIMO_DIAS));
        assert_eq!(plazo(&MAXIMO_DIAS.to_string()), Ok(MAXIMO_DIAS));
    }

    #[test]
    fn el_corte_deja_fuera_lo_viejo_y_conserva_el_borde() {
        let c = corte(t0(), 90);
        assert_eq!(c, t0() - Duration::days(90));
        // Lo de hace exactamente 90 días NO se borra: está dentro del plazo.
        assert!(!(t0() - Duration::days(90) < c));
        assert!(t0() - Duration::days(91) < c);
    }
}
