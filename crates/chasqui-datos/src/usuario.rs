// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Quién entra al panel, y el freno al ensayo de contraseñas.
//!
//! Las contraseñas se guardan con Argon2 (`guaca::claves`) y **la regla del
//! freno vive en `guaca::freno`**, no aquí: lo que diverge cuando se copia no es
//! el `INSERT`, es el umbral y qué borra la cuenta de fallos. Esta capa trae las
//! filas; guaca decide.

use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, SqlitePool};

use crate::{Error, Resultado};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usuario {
    pub id: i64,
    pub correo: String,
    pub nombre: String,
}

/// Da de alta a una persona del negocio del cliente.
pub async fn crear(
    pool: &SqlitePool,
    correo: &str,
    nombre: &str,
    clave: &str,
    ahora: DateTime<Utc>,
) -> Resultado<i64> {
    let hash = guaca::claves::hashear(clave).ok_or(Error::ClaveNoHasheable)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO usuario (correo, nombre, hash, activo, creado_en)
         VALUES (?1, ?2, ?3, 1, ?4) RETURNING id",
    )
    // El correo se guarda en minúsculas: el índice único ya lo trata sin
    // distinguir, y guardarlo como lo tecleó cada uno haría que la lista del
    // panel enseñara «Ana@…» y «ana@…» como si fueran dos personas.
    .bind(correo.trim().to_lowercase())
    .bind(nombre)
    .bind(hash)
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Cambia la contraseña de alguien.
pub async fn cambiar_clave(pool: &SqlitePool, id: i64, clave: &str) -> Resultado<()> {
    let hash = guaca::claves::hashear(clave).ok_or(Error::ClaveNoHasheable)?;
    sqlx::query("UPDATE usuario SET hash = ?1 WHERE id = ?2")
        .bind(hash)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// El hash contra el que se comprueba cuando el correo NO existe.
///
/// Se calcula una vez, con el MISMO `hashear` que produce los de verdad. No se
/// escribe a mano en una constante a propósito: si `guaca` cambiara los
/// parámetros de Argon2, un hash empotrado se seguiría verificando con los
/// viejos —más baratos— y la diferencia de tiempo volvería sin que nada lo
/// dijera. Derivarlo los mantiene atados.
fn senuelo() -> &'static str {
    static SENUELO: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SENUELO.get_or_init(|| {
        guaca::claves::hashear("una contraseña que no es la de nadie, solo para gastar el tiempo")
            .expect("hashear una constante no falla")
    })
}

/// Comprueba correo y contraseña. `None` si no casan.
///
/// **No distingue «no existe» de «contraseña mal»**, y no es pereza: decirlo
/// convierte la pantalla de entrada en un comprobador de qué correos tienen
/// cuenta en el negocio del cliente.
///
/// # Y no lo distingue TAMPOCO por el tiempo
///
/// Esa frase era cierta en el mensaje y falsa en el reloj. Medido el 2026-08-02
/// con la revisión del MVP: un correo que existe tardaba **426 ms** —lo que
/// cuesta Argon2— y uno que no existe **0,33 ms**, porque salía antes de
/// comprobar nada. **1292 veces más rápido**, o sea que la pantalla de entrada
/// sí era un comprobador de correos: no por lo que respondía, por lo que
/// tardaba en responderlo.
///
/// Así que cuando no hay fila se verifica igual, contra [`senuelo`]. El trabajo
/// se gasta a propósito, y `black_box` está para que el optimizador no se lleve
/// por delante una llamada cuyo resultado no se mira.
pub async fn autenticar(pool: &SqlitePool, correo: &str, clave: &str) -> Resultado<Option<Usuario>> {
    let fila = sqlx::query(
        "SELECT id, correo, nombre, hash FROM usuario WHERE lower(correo) = ?1 AND activo = 1",
    )
    .bind(correo.trim().to_lowercase())
    .fetch_optional(pool)
    .await?;

    let Some(fila) = fila else {
        std::hint::black_box(guaca::claves::verificar(clave, senuelo()));
        return Ok(None);
    };
    let hash: String = fila.try_get("hash")?;

    if !guaca::claves::verificar(clave, &hash) {
        return Ok(None);
    }

    Ok(Some(Usuario {
        id: fila.try_get("id")?,
        correo: fila.try_get("correo")?,
        nombre: fila.try_get("nombre")?,
    }))
}

/// Una persona por su id. Para resolver la sesión en cada petición.
pub async fn por_id(pool: &SqlitePool, id: i64) -> Resultado<Option<Usuario>> {
    let fila = sqlx::query("SELECT id, correo, nombre FROM usuario WHERE id = ?1 AND activo = 1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    fila.map(|f| {
        Ok(Usuario { id: f.try_get("id")?, correo: f.try_get("correo")?, nombre: f.try_get("nombre")? })
    })
    .transpose()
}

pub async fn listar(pool: &SqlitePool) -> Resultado<Vec<(i64, String, String, bool)>> {
    let filas = sqlx::query("SELECT id, correo, nombre, activo FROM usuario ORDER BY id")
        .fetch_all(pool)
        .await?;
    filas
        .into_iter()
        .map(|f| {
            let activo: i64 = f.try_get("activo")?;
            Ok((f.try_get("id")?, f.try_get("correo")?, f.try_get("nombre")?, activo != 0))
        })
        .collect()
}

// --- El freno ---

/// Anota un intento y poda lo viejo.
pub async fn anotar_intento(
    pool: &SqlitePool,
    correo: &str,
    origen: &str,
    exito: bool,
    ahora: DateTime<Utc>,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO intento_acceso (correo, origen, exito, momento) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(correo.trim().to_lowercase())
    .bind(origen)
    .bind(i64::from(exito))
    .bind(ahora.to_rfc3339())
    .execute(pool)
    .await?;

    // El borde sale de guaca y no de una resta local: con dos bordes de la misma
    // regla, en un año nadie sabe cuál manda.
    let corte = ahora - Duration::hours(guaca::freno::RETENCION_HORAS);
    sqlx::query("DELETE FROM intento_acceso WHERE momento < ?1")
        .bind(corte.to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// El estado del freno para este par (correo, origen).
pub async fn estado_freno(
    pool: &SqlitePool,
    correo: &str,
    origen: &str,
    ahora: DateTime<Utc>,
) -> Resultado<guaca::freno::Estado> {
    let filas: Vec<(String, i64)> = sqlx::query_as(
        "SELECT momento, exito FROM intento_acceso
         WHERE correo = ?1 AND origen = ?2 ORDER BY momento",
    )
    .bind(correo.trim().to_lowercase())
    .bind(origen)
    .fetch_all(pool)
    .await?;

    // Las marcas de tiempo van SIN parsear: qué hacer con una ilegible es una
    // decisión de seguridad, y guaca la toma para que no la tome cada app.
    let intentos: Vec<guaca::freno::Intento<'_>> = filas
        .iter()
        .map(|(momento, exito)| guaca::freno::Intento { momento, exito: *exito != 0 })
        .collect();

    Ok(guaca::freno::evaluar(&intentos, ahora))
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::pruebas::base_de_prueba;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn se_entra_con_la_contraseña_correcta() {
        let pool = base_de_prueba().await;
        crear(&pool, "ana@acme.com", "Ana", "una-contraseña-larga", t0()).await.unwrap();

        let u = autenticar(&pool, "ana@acme.com", "una-contraseña-larga").await.unwrap().unwrap();
        assert_eq!(u.nombre, "Ana");
    }

    #[tokio::test]
    async fn la_contraseña_no_esta_en_la_base() {
        let pool = base_de_prueba().await;
        crear(&pool, "ana@acme.com", "Ana", "una-contraseña-larga", t0()).await.unwrap();

        let hash: String = sqlx::query_scalar("SELECT hash FROM usuario").fetch_one(&pool).await.unwrap();
        assert!(!hash.contains("una-contraseña-larga"));
        assert!(hash.starts_with("$argon2"), "no es un hash de Argon2: {hash}");
    }

    #[tokio::test]
    async fn con_la_contraseña_mal_no_se_entra() {
        let pool = base_de_prueba().await;
        crear(&pool, "ana@acme.com", "Ana", "una-contraseña-larga", t0()).await.unwrap();
        assert!(autenticar(&pool, "ana@acme.com", "otra-cosa").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn un_correo_que_no_existe_falla_igual_que_una_contraseña_mala() {
        // Las dos devuelven `None`. Distinguirlas convertiría la pantalla de
        // entrada en un comprobador de quién tiene cuenta.
        let pool = base_de_prueba().await;
        assert!(autenticar(&pool, "nadie@acme.com", "lo-que-sea").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn el_correo_no_distingue_mayusculas() {
        let pool = base_de_prueba().await;
        crear(&pool, "Ana@Acme.com", "Ana", "una-contraseña-larga", t0()).await.unwrap();
        assert!(autenticar(&pool, "ana@acme.com", "una-contraseña-larga").await.unwrap().is_some());

        // Y el mismo correo con otras mayúsculas no es una segunda cuenta.
        assert!(crear(&pool, "ANA@ACME.COM", "Otra", "x", t0()).await.is_err());
    }

    #[tokio::test]
    async fn cambiar_la_clave_invalida_la_anterior() {
        let pool = base_de_prueba().await;
        let id = crear(&pool, "ana@acme.com", "Ana", "la-vieja-larga", t0()).await.unwrap();
        cambiar_clave(&pool, id, "la-nueva-larga").await.unwrap();

        assert!(autenticar(&pool, "ana@acme.com", "la-vieja-larga").await.unwrap().is_none());
        assert!(autenticar(&pool, "ana@acme.com", "la-nueva-larga").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn el_freno_aparece_tras_cinco_fallos_y_un_acierto_lo_borra() {
        let pool = base_de_prueba().await;
        let (correo, origen) = ("ana@acme.com", "10.0.0.1");

        assert!(!estado_freno(&pool, correo, origen, t0()).await.unwrap().frenado());

        for i in 0..guaca::freno::UMBRAL {
            anotar_intento(&pool, correo, origen, false, t0() + Duration::seconds(i)).await.unwrap();
        }
        let estado = estado_freno(&pool, correo, origen, t0() + Duration::seconds(10)).await.unwrap();
        assert!(estado.frenado(), "cinco fallos y no hay freno");

        anotar_intento(&pool, correo, origen, true, t0() + Duration::minutes(10)).await.unwrap();
        assert!(
            !estado_freno(&pool, correo, origen, t0() + Duration::minutes(11)).await.unwrap().frenado(),
            "un acierto tiene que borrar la cuenta de fallos"
        );
    }

    #[tokio::test]
    async fn el_freno_es_por_par_y_no_deja_fuera_a_quien_no_ha_fallado() {
        // La prueba de la directiva 21: la restricción contra la operación
        // normal. Si el freno fuera solo por correo, un desconocido dejaría
        // fuera a Ana desde su casa.
        let pool = base_de_prueba().await;
        for i in 0..10 {
            anotar_intento(&pool, "ana@acme.com", "10.0.0.9", false, t0() + Duration::seconds(i))
                .await
                .unwrap();
        }
        let ana = estado_freno(&pool, "ana@acme.com", "10.0.0.1", t0() + Duration::minutes(1))
            .await
            .unwrap();
        assert!(!ana.frenado(), "Ana se quedó fuera por los fallos de otro");
    }

    #[tokio::test]
    async fn los_intentos_viejos_se_podan() {
        let pool = base_de_prueba().await;
        anotar_intento(&pool, "ana@acme.com", "10.0.0.1", false, t0()).await.unwrap();
        anotar_intento(
            &pool,
            "ana@acme.com",
            "10.0.0.1",
            false,
            t0() + Duration::hours(guaca::freno::RETENCION_HORAS + 1),
        )
        .await
        .unwrap();

        let cuantos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM intento_acceso")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cuantos, 1, "la tabla crece sin límite");
    }
}

#[cfg(test)]
mod pruebas_de_tiempo {
    use super::*;
    use crate::pruebas::base_de_prueba;

    /// **El reloj también responde.** Sin el señuelo de [`autenticar`], un correo
    /// que no existe salía sin pagar Argon2: medido el 2026-08-02, 426 ms contra
    /// 0,33 ms — **1292 veces más rápido**. Con eso, la pantalla de entrada es un
    /// comprobador de qué correos tienen cuenta en el negocio del cliente, por
    /// mucho que las dos respuestas digan lo mismo.
    ///
    /// El umbral es flojo —un tercio— a propósito: lo que hay que cazar es el
    /// regreso al camino rápido, que son tres órdenes de magnitud, no una
    /// diferencia del 20 % que en una máquina cargada es ruido. Una prueba de
    /// tiempo apretada es una prueba que falla sola y que se acaba borrando.
    #[tokio::test]
    async fn un_correo_que_no_existe_tarda_LO_MISMO_que_uno_que_si() {
        let pool = base_de_prueba().await;
        crear(&pool, "ana@acme.com", "Ana", "una-contraseña-larga", Utc::now()).await.unwrap();

        // Se calienta el señuelo: la primera llamada paga además su OnceLock, y
        // medir eso sería medir el arranque, no el camino.
        autenticar(&pool, "nadie@acme.com", "x").await.unwrap();

        const VUELTAS: u32 = 4;
        let mut existe = std::time::Duration::ZERO;
        let mut no_existe = std::time::Duration::ZERO;
        for _ in 0..VUELTAS {
            let t = std::time::Instant::now();
            autenticar(&pool, "ana@acme.com", "la-que-no-es").await.unwrap();
            existe += t.elapsed();

            let t = std::time::Instant::now();
            autenticar(&pool, "nadie@acme.com", "la-que-no-es").await.unwrap();
            no_existe += t.elapsed();
        }

        assert!(
            no_existe * 3 >= existe,
            "el correo que no existe tardó {:?} y el que sí {:?}: por ahí se enumera quién tiene \
             cuenta, sin que las respuestas se distingan en nada más",
            no_existe / VUELTAS,
            existe / VUELTAS
        );
    }
}
