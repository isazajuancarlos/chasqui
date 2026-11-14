// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Juan Carlos Isaza
//! El freno al ensayo de contraseñas.
//!
//! # La regla, en una frase
//!
//! Tras cinco fallos seguidos desde el mismo origen contra el mismo correo, el
//! siguiente intento espera; la espera se dobla con cada fallo y se detiene a los
//! cinco minutos. Un acierto lo borra todo.
//!
//! # Lo que se rechazó al diseñarla
//!
//! **Bloquear la cuenta.** Un bloqueo que exige a alguien desbloquear convierte
//! cinco peticiones de un desconocido en una historia clínica inaccesible a las
//! tres de la mañana. La regla se probó contra «¿qué cliente legítimo necesita
//! justo esto?»: la respuesta es que ninguno necesita quedarse fuera, y todos
//! necesitan que el diccionario no avance.
//!
//! **Contar solo por cuenta.** Con la cuenta sola, el atacante remoto elige a
//! quién dejar fuera. Con el origen dentro de la clave se frena a sí mismo.
//!
//! **Un tope alto y creciente sin límite.** Doblar sin techo es un bloqueo con
//! otro nombre: a los quince fallos serían horas. El techo son cinco minutos, y
//! eso ya baja un diccionario a menos de trescientos intentos al día.
//!
//! # Lo que este freno NO resuelve, y no debe aparentar que sí
//!
//! El relleno de credenciales desde muchas máquinas contra muchas cuentas: cada
//! par (correo, origen) falla una vez y ninguno llega al umbral. Contra eso la
//! respuesta es el segundo factor, no un freno más agresivo — que solo añadiría
//! el bloqueo que aquí se rechazó. Este módulo es el prerrequisito de ese
//! segundo factor, no su sustituto.
//!
//! # Por qué la decisión vive aquí y el almacenamiento no
//!
//! guaca no depende de ninguna base de datos, y meterle `sqlx` para esto la
//! convertiría en otra cosa. Pero lo que diverge cuando se copia no es el
//! `INSERT`: es *la regla* —el umbral, el techo, qué borra la cuenta de fallos,
//! qué se hace con una marca de tiempo ilegible—. Así que la app trae sus filas
//! y [`evaluar`] decide. La app pone dos consultas triviales; guaca pone todo lo
//! que hay que auditar una vez y no dos.

use chrono::{DateTime, Duration, Utc};

/// Fallos seguidos que se admiten sin espera.
///
/// Cinco: equivocarse tres o cuatro veces al teclear es corriente, y a la quinta
/// ya no lo es. El sexto intento espera quince segundos, que un humano ni
/// registra como castigo y a un programa le arruina el ritmo.
pub const UMBRAL: i64 = 5;

/// Espera tras el primer fallo por encima del umbral. Se dobla con cada uno.
pub const BASE_SEGUNDOS: i64 = 15;

/// Techo de la espera. Ver el módulo: sin techo, el freno es un bloqueo.
pub const TOPE_SEGUNDOS: i64 = 300;

/// Cuánto se conserva un intento. Lo anterior no dice nada del ritmo actual y
/// solo haría crecer la tabla sin límite.
pub const RETENCION_HORAS: i64 = 24;

/// Un intento tal y como sale de la base, con la marca de tiempo **sin parsear**.
///
/// Se recibe en crudo a propósito: qué hacer con una marca ilegible es una
/// decisión de seguridad (ver [`evaluar`]), y si la app parsea antes de llamar,
/// esa decisión se la queda la app — que es justo lo que este módulo existe para
/// evitar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intento<'a> {
    /// Instante del intento en RFC 3339.
    pub momento: &'a str,
    pub exito: bool,
}

/// El estado del freno para un (correo, origen) en un instante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estado {
    /// Fallos seguidos desde el último acierto (o desde el borde de retención).
    pub fallos: i64,
    /// Cuánto falta para poder volver a probar. Cero = adelante.
    pub espera: Duration,
}

impl Estado {
    pub fn frenado(&self) -> bool {
        self.espera > Duration::zero()
    }

    /// Segundos que faltan, redondeados hacia arriba, para la cabecera
    /// `Retry-After`. Nunca cero cuando hay freno: un `Retry-After: 0` invita a
    /// reintentar de inmediato, que es lo contrario de lo que se pide.
    pub fn segundos_restantes(&self) -> u64 {
        if !self.frenado() {
            return 0;
        }
        // Redondeo hacia arriba a mano: `div_ceil` sobre enteros con signo
        // todavía no está estabilizado y esto no justifica exigir una versión
        // concreta del compilador.
        let ms = self.espera.num_milliseconds().max(1);
        ms.div_euclid(1000).saturating_add(if ms % 1000 == 0 { 0 } else { 1 }).max(1) as u64
    }
}

/// La espera que corresponde a `fallos` fallos seguidos. Función pura: es la
/// regla, y se prueba sin base de datos.
pub fn espera_de(fallos: i64) -> Duration {
    // `<` y no `<=`: con `<=`, cinco fallos dejaban pasar un sexto intento libre
    // y el freno no aparecía hasta el séptimo. La diferencia es un intento, pero
    // era un intento entre lo que dice este módulo y lo que hacía — y dos
    // maneras de contar la misma regla garantizan que dentro de un año nadie
    // sepa cuál era. Lo cazó la prueba de la RUTA, no las de aquí.
    if fallos < UMBRAL {
        return Duration::zero();
    }
    // `saturating_*` y el techo antes de convertir: con 64 fallos, `1 << 63` ya
    // es desbordamiento, y un desbordamiento aquí daría una espera negativa —
    // es decir, ningún freno justo cuando más se está intentando.
    let exceso = (fallos - UMBRAL).min(32) as u32;
    let segundos = BASE_SEGUNDOS.saturating_mul(1i64 << exceso).min(TOPE_SEGUNDOS);
    Duration::seconds(segundos)
}

/// El estado del freno a partir de los intentos de UN par (correo, origen).
///
/// La app entrega lo que tiene guardado para esa pareja, en cualquier orden; aquí
/// se aplica la retención, se busca el último acierto y se cuentan los fallos
/// posteriores.
///
/// # Las dos decisiones que no se delegan
///
/// **Una marca de tiempo ilegible no abre la puerta.** Un fallo con fecha
/// irreconocible se trata como si acabara de ocurrir —se cumple la espera
/// entera—, y un *acierto* ilegible NO borra la cuenta de fallos. Las dos van en
/// la dirección conservadora: el camino permisivo ante un dato roto es el que
/// convierte un fallo de datos en una vía de entrada.
///
/// **La retención se aplica aquí también**, aunque la app pode al escribir: si un
/// día la poda falla o alguien cambia el `DELETE`, el freno no debe empezar a
/// contar fallos de anteayer.
pub fn evaluar(intentos: &[Intento<'_>], ahora: DateTime<Utc>) -> Estado {
    let corte = ahora - Duration::hours(RETENCION_HORAS);

    // Un acierto que no se puede leer se ignora — no borra nada. Un fallo que no
    // se puede leer se cuenta como recién ocurrido.
    let leer = |m: &str| DateTime::parse_from_rfc3339(m).map(|t| t.with_timezone(&Utc));

    let ultimo_exito = intentos
        .iter()
        .filter(|i| i.exito)
        .filter_map(|i| leer(i.momento).ok())
        .filter(|t| *t >= corte)
        .max();
    let desde = ultimo_exito.unwrap_or(corte);

    let fallidos: Vec<DateTime<Utc>> = intentos
        .iter()
        .filter(|i| !i.exito)
        .map(|i| leer(i.momento).unwrap_or(ahora))
        .filter(|t| *t > desde)
        .collect();

    let fallos = fallidos.len() as i64;
    let espera = match fallidos.iter().max() {
        None => Duration::zero(),
        Some(ultimo) => {
            let castigo = espera_de(fallos);
            if castigo.is_zero() {
                Duration::zero()
            } else {
                let libre = *ultimo + castigo;
                if libre > ahora { libre - ahora } else { Duration::zero() }
            }
        }
    };

    Estado { fallos, espera }
}

/// El instante a partir del cual una fila deja de importar: lo que la app debe
/// podar al anotar, y nada más viejo que eso.
///
/// Existe para que el `DELETE` de la app y el conteo de [`evaluar`] usen el mismo
/// borde. Con dos bordes distintos, el freno cuenta filas que la poda ya
/// consideraba muertas — o al revés.
pub fn corte_retencion(ahora: DateTime<Utc>) -> DateTime<Utc> {
    ahora - Duration::hours(RETENCION_HORAS)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Los intentos de una prueba: `(desplazamiento en segundos, exito)`.
    fn intentos(t0: DateTime<Utc>, filas: &[(i64, bool)]) -> Vec<String> {
        filas
            .iter()
            .map(|(s, _)| (t0 + Duration::seconds(*s)).to_rfc3339())
            .collect()
    }

    fn vista<'a>(marcas: &'a [String], filas: &[(i64, bool)]) -> Vec<Intento<'a>> {
        marcas
            .iter()
            .zip(filas)
            .map(|(m, (_, exito))| Intento { momento: m.as_str(), exito: *exito })
            .collect()
    }

    #[test]
    fn hasta_el_umbral_no_hay_espera_y_despues_se_dobla() {
        // Los cinco fallos admitidos se responden sin espera; a partir del
        // quinto, el intento siguiente espera. El borde exacto está aquí y en la
        // prueba de la ruta de cada app, y tienen que decir lo mismo.
        for n in 0..UMBRAL {
            assert!(espera_de(n).is_zero(), "{n} fallos no deberían esperar");
        }
        assert_eq!(espera_de(UMBRAL).num_seconds(), BASE_SEGUNDOS);
        assert_eq!(espera_de(UMBRAL + 1).num_seconds(), BASE_SEGUNDOS * 2);
        assert_eq!(espera_de(UMBRAL + 2).num_seconds(), BASE_SEGUNDOS * 4);
    }

    #[test]
    fn la_espera_tiene_techo_y_no_se_desborda() {
        assert_eq!(espera_de(100).num_seconds(), TOPE_SEGUNDOS);
        // El caso que un `1 << n` sin cuidado convertiría en espera NEGATIVA, o
        // sea en ausencia de freno justo con el atacante más insistente.
        assert_eq!(espera_de(i64::MAX).num_seconds(), TOPE_SEGUNDOS);
        assert!(espera_de(i64::MAX) > Duration::zero());
    }

    #[test]
    fn cuatro_fallos_no_frenan_y_el_quinto_si() {
        let t0 = Utc::now();
        let filas: Vec<(i64, bool)> = (0..(UMBRAL - 1)).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let e = evaluar(&vista(&m, &filas), t0 + Duration::seconds(UMBRAL));
        assert_eq!(e.fallos, UMBRAL - 1);
        assert!(!e.frenado(), "cuatro fallos ya frenaban: eso castiga a quien teclea mal");

        let filas: Vec<(i64, bool)> = (0..UMBRAL).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let e = evaluar(&vista(&m, &filas), t0 + Duration::seconds(UMBRAL));
        assert!(e.frenado(), "el quinto fallo no frenó el intento siguiente");
        assert!(e.segundos_restantes() >= 1);
    }

    #[test]
    fn entrar_bien_borra_la_cuenta_de_fallos() {
        let t0 = Utc::now();
        let mut filas: Vec<(i64, bool)> = (0..10).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        assert!(evaluar(&vista(&m, &filas), t0 + Duration::seconds(11)).frenado());

        filas.push((600, true));
        let m = intentos(t0, &filas);
        let e = evaluar(&vista(&m, &filas), t0 + Duration::seconds(601));
        assert_eq!(e.fallos, 0, "los fallos previos al acierto siguen contando");
        assert!(!e.frenado());
    }

    #[test]
    fn los_fallos_posteriores_al_acierto_si_cuentan() {
        // El reverso de la prueba anterior, y la que discrimina: un `evaluar` que
        // se limitara a devolver cero en cuanto ve un acierto pasaría aquella y
        // fallaría esta.
        let t0 = Utc::now();
        let mut filas: Vec<(i64, bool)> = vec![(0, true)];
        filas.extend((1..=UMBRAL).map(|i| (i, false)));
        let m = intentos(t0, &filas);
        let e = evaluar(&vista(&m, &filas), t0 + Duration::seconds(UMBRAL + 1));
        assert_eq!(e.fallos, UMBRAL);
        assert!(e.frenado(), "entrar bien una vez no puede dar barra libre después");
    }

    #[test]
    fn la_espera_se_agota_sola() {
        let t0 = Utc::now();
        // Justo los fallos del umbral: la espera es la base, contada desde el
        // ÚLTIMO fallo. Con más fallos la espera se dobla y este cálculo tendría
        // que doblarse también — de ahí que las cuentas salgan de las constantes.
        let filas: Vec<(i64, bool)> = (0..UMBRAL).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let ultimo_fallo = t0 + Duration::seconds(UMBRAL - 1);
        assert!(evaluar(&vista(&m, &filas), ultimo_fallo).frenado());
        // Pasada la espera, se puede volver a probar SIN que nadie desbloquee
        // nada: es lo que separa un freno de un bloqueo.
        let luego = ultimo_fallo + Duration::seconds(BASE_SEGUNDOS + 1);
        assert!(!evaluar(&vista(&m, &filas), luego).frenado());
    }

    #[test]
    fn lo_anterior_a_la_retencion_no_cuenta() {
        let t0 = Utc::now();
        // Veinte fallos de hace dos días no dicen nada del ritmo de hoy.
        let filas: Vec<(i64, bool)> =
            (0..20).map(|i| (-(RETENCION_HORAS * 3600) - 60 + i, false)).collect();
        let m = intentos(t0, &filas);
        let e = evaluar(&vista(&m, &filas), t0);
        assert_eq!(e.fallos, 0, "el freno contó fallos fuera de la retención");
        assert!(!e.frenado());
    }

    #[test]
    fn un_fallo_con_marca_ilegible_se_cuenta_y_frena() {
        // Directiva: ante un dato roto, el camino conservador. Si una marca
        // irreconocible se descartara, bastaría con corromper la columna para
        // apagar el freno.
        let t0 = Utc::now();
        let ilegibles: Vec<Intento<'_>> =
            (0..UMBRAL).map(|_| Intento { momento: "no es una fecha", exito: false }).collect();
        let e = evaluar(&ilegibles, t0);
        assert_eq!(e.fallos, UMBRAL);
        assert!(e.frenado(), "una marca ilegible apagó el freno");
        assert_eq!(e.segundos_restantes(), BASE_SEGUNDOS as u64);
    }

    #[test]
    fn un_acierto_con_marca_ilegible_no_borra_los_fallos() {
        // La otra mitad de la misma decisión, y la que no es obvia: si un acierto
        // ilegible se tratara como «ahora», borraría la cuenta de fallos — o sea,
        // corromper UNA fila apagaría el freno igualmente.
        let t0 = Utc::now();
        let filas: Vec<(i64, bool)> = (0..UMBRAL).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let mut v = vista(&m, &filas);
        v.push(Intento { momento: "tampoco es una fecha", exito: true });

        let e = evaluar(&v, t0 + Duration::seconds(UMBRAL));
        assert_eq!(e.fallos, UMBRAL, "un acierto ilegible borró los fallos");
        assert!(e.frenado());
    }

    #[test]
    fn el_orden_de_las_filas_no_cambia_el_veredicto() {
        // La app entrega lo que le devuelva su consulta; si el resultado
        // dependiera del `ORDER BY`, cambiar la consulta cambiaría la seguridad.
        let t0 = Utc::now();
        let filas: Vec<(i64, bool)> = (0..UMBRAL).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let ascendente = vista(&m, &filas);
        let mut descendente = ascendente.clone();
        descendente.reverse();

        let ahora = t0 + Duration::seconds(UMBRAL);
        assert_eq!(evaluar(&ascendente, ahora), evaluar(&descendente, ahora));
    }

    #[test]
    fn sin_intentos_no_hay_freno() {
        assert!(!evaluar(&[], Utc::now()).frenado());
    }

    #[test]
    fn el_corte_de_retencion_es_el_mismo_que_usa_evaluar() {
        // Los dos bordes tienen que ser uno. Un fallo justo DENTRO del corte
        // cuenta; el mismo fallo un segundo antes del corte, no.
        let ahora = Utc::now();
        let corte = corte_retencion(ahora);

        let dentro = (corte + Duration::seconds(1)).to_rfc3339();
        let fuera = (corte - Duration::seconds(1)).to_rfc3339();
        assert_eq!(evaluar(&[Intento { momento: &dentro, exito: false }], ahora).fallos, 1);
        assert_eq!(evaluar(&[Intento { momento: &fuera, exito: false }], ahora).fallos, 0);
    }

    /// Directiva #8: la regla evaluada sobre un volumen grande no se desborda, no
    /// se vuelve permisiva y no depende del orden. Con plazo, para que un cuelgue
    /// no se lea como lentitud.
    #[test]
    fn ciento_veinte_intentos() {
        let t0 = Utc::now();
        let filas: Vec<(i64, bool)> = (0..120).map(|i| (i, false)).collect();
        let m = intentos(t0, &filas);
        let v = vista(&m, &filas);

        let inicio = std::time::Instant::now();
        let e = evaluar(&v, t0 + Duration::seconds(120));
        assert!(inicio.elapsed() < std::time::Duration::from_secs(5), "evaluar tardó demasiado");

        assert_eq!(e.fallos, 120, "se perdieron intentos");
        assert_eq!(espera_de(e.fallos).num_seconds(), TOPE_SEGUNDOS, "el castigo no llegó al tope");
        assert!(e.frenado(), "120 fallos seguidos no dejaron freno puesto");
        // `espera` es lo que FALTA, no el castigo: el último fallo fue en t0+119
        // y se consulta en t0+120, así que el restante es el tope menos un
        // segundo. Comparar el restante con el tope era comparar dos magnitudes
        // distintas.
        assert_eq!(e.espera.num_seconds(), TOPE_SEGUNDOS - 1);
    }
}
