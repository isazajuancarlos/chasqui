// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Poner nombre a unas coordenadas, **sin preguntarle a nadie**.
//!
//! # Por qué está aquí dentro y no es una llamada a un mapa
//!
//! Que el panel diga «Amagá, Antioquia, Colombia» al lado de un punto parece un
//! detalle de presentación, y es una decisión de producto: para traducir unas
//! coordenadas preguntándole a Google o a Mapbox habría que **mandarles la
//! ubicación exacta de un cliente del cliente**, en cada apertura de la
//! conversación. Eso rompe la frase con la que Chasqui se vende —«las
//! conversaciones no salen de su servidor»— y convierte al cliente en
//! responsable de una transferencia internacional que no declaró.
//!
//! Reducir lo que se ENSEÑA no arregla eso, porque lo que se TRANSMITE sigue
//! siendo el punto exacto. Así que la tabla viaja dentro del ejecutable y la
//! búsqueda es aritmética. Cero peticiones, y funciona sin internet, que es lo
//! que se le promete al cliente.
//!
//! # La etiqueta lleva su propio error
//!
//! **Medido el 2026-08-02** sobre 300 pueblos de menos de 5 000 habitantes de
//! Latinoamérica: la ciudad conocida más cercana está a 10,3 km de mediana, a
//! 34,5 km en el percentil 90 y hasta a 121 km en el peor caso. Uno de cada seis
//! puntos nombraría un pueblo a más de 25 km **como si fuera donde está el
//! cliente**: plausible y falso, que es la peor clase de dato.
//!
//! Por eso [`Lugar`] lleva la distancia y quien lo enseña la dice. Y pasado
//! [`TOPE_KM`] no se etiqueta: se admite que no se sabe (directiva 20), porque
//! nombrar una ciudad a 300 km no informa, engaña.
//!
//! # Los datos
//!
//! GeoNames `cities5000`, bajo **CC BY 4.0** — la atribución es obligatoria y
//! está en `NOTICE`. El nivel se eligió midiendo: con `cities15000` un punto
//! real de Amagá salía como «Caldas», que es otro municipio.

use std::sync::OnceLock;

/// Más allá de esto no se pone nombre. Cien kilómetros es más que el peor caso
/// medido para un pueblo pequeño (121 km), así que el corte deja fuera los
/// puntos en mitad del mar o del desierto sin quitarle la etiqueta a nadie que
/// viva en un sitio.
pub const TOPE_KM: f64 = 100.0;

/// Dónde cae un punto, con lo que se sabe y con lo que no.
#[derive(Debug, Clone, PartialEq)]
pub struct Lugar {
    pub ciudad: String,
    /// Departamento, estado o provincia. Vacío cuando GeoNames no lo tiene.
    pub division: String,
    pub pais: String,
    /// Distancia del punto a esa ciudad. **Es parte del dato, no un detalle**:
    /// sin ella «Amagá» y «Amagá a 40 km» se leen igual.
    pub km: f64,
}

struct Tabla {
    textos: &'static [u8],
    ciudades: &'static [u8],
    n: usize,
    divisiones: Vec<(u32, u8)>,
    paises: Vec<(u32, u8)>,
}

const BRUTO: &[u8] = include_bytes!("../datos/ciudades.bin");
const REGISTRO: usize = 16;
static TABLA: OnceLock<Option<Tabla>> = OnceLock::new();

fn u32_en(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn leer() -> &'static Option<Tabla> {
    TABLA.get_or_init(|| {
        // Un archivo que no cuadra NO se lee a medias: se apaga la función
        // entera y las ubicaciones salen con sus coordenadas y sin nombre. Leer
        // basura produciría ciudades inventadas, que es peor que no decir nada.
        if BRUTO.len() < 14 || &BRUTO[..6] != b"CHGEO1" {
            return None;
        }
        let n = u32_en(BRUTO, 6) as usize;
        let largo_textos = u32_en(BRUTO, 10) as usize;
        let ini_textos = 14;
        let ini_ciudades = ini_textos + largo_textos;
        let fin_ciudades = ini_ciudades + n * REGISTRO;
        if BRUTO.len() < fin_ciudades + 4 {
            return None;
        }

        let mut i = fin_ciudades;
        let leer_tabla = |i: &mut usize| -> Option<Vec<(u32, u8)>> {
            if BRUTO.len() < *i + 4 {
                return None;
            }
            let cuantos = u32_en(BRUTO, *i) as usize;
            *i += 4;
            if BRUTO.len() < *i + cuantos * 5 {
                return None;
            }
            let mut v = Vec::with_capacity(cuantos);
            for k in 0..cuantos {
                let p = *i + k * 5;
                v.push((u32_en(BRUTO, p), BRUTO[p + 4]));
            }
            *i += cuantos * 5;
            Some(v)
        };

        let divisiones = leer_tabla(&mut i)?;
        let paises = leer_tabla(&mut i)?;

        Some(Tabla {
            textos: &BRUTO[ini_textos..ini_ciudades],
            ciudades: &BRUTO[ini_ciudades..fin_ciudades],
            n,
            divisiones,
            paises,
        })
    })
}

impl Tabla {
    fn texto(&self, off: u32, largo: u8) -> String {
        let a = off as usize;
        let b = a + largo as usize;
        match self.textos.get(a..b) {
            Some(t) => String::from_utf8_lossy(t).into_owned(),
            None => String::new(),
        }
    }
}

/// Cuántos kilómetros hay entre dos puntos, aproximando la Tierra por un plano
/// local.
///
/// Sobra para esto: comparar distancias de decenas de kilómetros para quedarse
/// con la menor. Lo que **no** se puede omitir es envolver la diferencia de
/// longitud: sin eso, un punto en 179° y una ciudad en −179° —que están a 220 km—
/// saldrían separados por media vuelta al mundo, y el antimeridiano se quedaría
/// sin etiquetas sin que nada lo explicara.
fn km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1) * 111.32;
    let mut dlon = lon2 - lon1;
    if dlon > 180.0 {
        dlon -= 360.0;
    } else if dlon < -180.0 {
        dlon += 360.0;
    }
    let dx = dlon * 111.32 * ((lat1 + lat2) / 2.0).to_radians().cos();
    (dlat * dlat + dx * dx).sqrt()
}

/// La ciudad conocida más cercana a un punto, o `None` si no hay ninguna a menos
/// de [`TOPE_KM`].
///
/// Recorre las 69 561 ciudades una por una. No hay índice espacial y no hace
/// falta: son unas decenas de microsegundos, ocurre una vez por mensaje con
/// ubicación —que son pocos— y un árbol de búsqueda añadiría un mecanismo que
/// habría que probar a cambio de nada medible (directiva 1).
pub fn situar(latitud: f64, longitud: f64) -> Option<Lugar> {
    if !latitud.is_finite() || !longitud.is_finite() {
        return None;
    }
    let tabla = leer().as_ref()?;

    let mut mejor: Option<(f64, usize)> = None;
    for k in 0..tabla.n {
        let p = k * REGISTRO;
        let lat = f32::from_le_bytes([
            tabla.ciudades[p],
            tabla.ciudades[p + 1],
            tabla.ciudades[p + 2],
            tabla.ciudades[p + 3],
        ]) as f64;
        let lon = f32::from_le_bytes([
            tabla.ciudades[p + 4],
            tabla.ciudades[p + 5],
            tabla.ciudades[p + 6],
            tabla.ciudades[p + 7],
        ]) as f64;
        let d = km(latitud, longitud, lat, lon);
        if mejor.is_none_or(|(m, _)| d < m) {
            mejor = Some((d, k));
        }
    }

    let (distancia, k) = mejor?;
    if distancia > TOPE_KM {
        return None;
    }

    let p = k * REGISTRO;
    let off = u32_en(tabla.ciudades, p + 8);
    let largo = tabla.ciudades[p + 12];
    let i_div = u16::from_le_bytes([tabla.ciudades[p + 13], tabla.ciudades[p + 14]]) as usize;
    let i_pais = tabla.ciudades[p + 15] as usize;

    Some(Lugar {
        ciudad: tabla.texto(off, largo),
        division: tabla
            .divisiones
            .get(i_div)
            .map(|(o, l)| tabla.texto(*o, *l))
            .unwrap_or_default(),
        pais: tabla.paises.get(i_pais).map(|(o, l)| tabla.texto(*o, *l)).unwrap_or_default(),
        km: distancia,
    })
}

impl Lugar {
    /// «Amagá, Antioquia, Colombia», saltándose lo que no se sabe.
    pub fn nombre(&self) -> String {
        [&self.ciudad, &self.division, &self.pais]
            .into_iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_archivo_empotrado_se_lee() {
        // Si esto falla, el generador y el lector dejaron de entenderse y TODAS
        // las ubicaciones saldrían sin nombre — en silencio, que es lo grave.
        let t = leer().as_ref().expect("el archivo de ciudades no se pudo leer");
        assert!(t.n > 60_000, "solo {} ciudades", t.n);
        assert!(t.paises.len() > 200);
    }

    #[test]
    fn el_punto_real_de_juan_cae_en_amaga() {
        // Las coordenadas que llegaron de WhatsApp el 2026-08-02.
        let l = situar(6.0265235, -75.6994566).expect("no situó un punto de Antioquia");
        assert_eq!(l.ciudad, "Amagá", "{l:?}");
        assert_eq!(l.division, "Antioquia");
        assert_eq!(l.pais, "Colombia");
        assert!(l.km < 5.0, "lo puso a {:.1} km", l.km);
    }

    #[test]
    fn en_mitad_del_pacifico_no_se_inventa_una_ciudad() {
        // Directiva 20: ante la ausencia, decirlo. Lo contrario sería etiquetar
        // un punto en alta mar con el nombre de un pueblo a 3 000 km.
        assert_eq!(situar(-30.0, -140.0), None);
    }

    #[test]
    fn funciona_en_los_cuatro_cuadrantes_y_no_solo_en_america() {
        for (lat, lon, esperado) in [
            (19.4326, -99.1332, "México"),
            (-34.6037, -58.3816, "Argentina"),
            (40.4168, -3.7038, "España"),
            (35.6895, 139.6917, "Japón"),
            (-33.8688, 151.2093, "Australia"),
            (-12.0464, -77.0428, "Perú"),
        ] {
            let l = situar(lat, lon).unwrap_or_else(|| panic!("no situó {esperado}"));
            assert_eq!(l.pais, esperado, "{l:?}");
        }
    }

    #[test]
    fn los_nombres_salen_en_castellano_y_no_en_ingles() {
        // GeoNames escribe en inglés. Un panel en castellano que diga «Mexico
        // City, Mexico» se lee como un producto a medio traducir, y México es
        // mercado. Los nombres vienen de `alternateNames` en `es` y los países
        // de iso-codes.
        let l = situar(19.4326, -99.1332).expect("no situó Ciudad de México");
        assert!(!l.ciudad.contains("Mexico City"), "quedó en inglés: {l:?}");
        assert_eq!(l.pais, "México");
        assert_eq!(situar(35.6895, 139.6917).unwrap().pais, "Japón");
    }

    #[test]
    fn la_distancia_envuelve_el_antimeridiano() {
        // 179 y −179 están a unos 200 km en esa latitud, no a media vuelta al
        // mundo. Sin envolver, el corte de 100 km dejaría sin etiqueta una
        // franja entera del planeta y nadie sabría por qué.
        let d = km(0.0, 179.0, 0.0, -179.0);
        assert!(d < 250.0, "midió {d:.0} km cruzando el antimeridiano");
    }

    #[test]
    fn situar_es_lo_bastante_rapido_para_hacerlo_por_mensaje() {
        // No es una prueba de rendimiento por gusto: si esto costara decenas de
        // milisegundos, habría que meterlo en otra tarea y el camino del webhook
        // —que tiene que contestarle 200 a Meta— se complicaría por una
        // etiqueta. Se mide para saber que NO hace falta.
        let inicio = std::time::Instant::now();
        for _ in 0..20 {
            situar(6.0265235, -75.6994566);
        }
        let por_llamada = inicio.elapsed() / 20;
        assert!(
            por_llamada < std::time::Duration::from_millis(20),
            "cada búsqueda tarda {por_llamada:?}"
        );
    }
}
