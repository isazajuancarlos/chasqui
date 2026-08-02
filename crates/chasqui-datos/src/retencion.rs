// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Borrar: por plazo vencido y a petición de la persona.
//!
//! Las dos cosas son el mismo mecanismo con distinto alcance, y **el alcance va
//! como parámetro, nunca como constante** (directiva 22): una limpieza que borra
//! más de lo que se pretendía no se nota en la primera vuelta, se nota en la
//! segunda, cuando falta el historial de alguien.
//!
//! Y las dos dejan constancia. Habeas data no se cumple borrando: se cumple
//! **pudiendo demostrar que se borró** el día que alguien reclama.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::Resultado;

/// Qué se borró.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct Resumen {
    pub conversaciones: i64,
    pub mensajes: i64,
}

impl Resumen {
    pub fn nada(&self) -> bool {
        self.conversaciones == 0 && self.mensajes == 0
    }
}

/// Borra todo lo de un contacto, por petición suya.
///
/// Es el derecho de supresión del artículo 8 de la Ley 1581. Se borra el
/// historial entero de esa persona en esta instancia, y queda la constancia.
pub async fn olvidar(
    pool: &SqlitePool,
    contacto: &str,
    ahora: DateTime<Utc>,
) -> Resultado<Resumen> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM conversacion WHERE contacto = ?1")
            .bind(contacto)
            .fetch_all(pool)
            .await?;

    let resumen = borrar(pool, &ids).await?;
    anotar(pool, contacto, "peticion", resumen, ahora).await?;
    Ok(resumen)
}

/// Borra lo que pasó del plazo.
///
/// `corte` viene calculado por `chasqui_dominio::retencion`: aquí no se decide
/// cuánto tiempo es «demasiado», solo se ejecuta. Si el plazo lo decidiera esta
/// función, habría dos sitios diciendo cuánto dura un dato.
pub async fn purgar(
    pool: &SqlitePool,
    corte: DateTime<Utc>,
    ahora: DateTime<Utc>,
) -> Resultado<Resumen> {
    // `ultimo_en` y no `abierta_en`: lo que cuenta es cuándo se tocó por última
    // vez. Con `abierta_en`, una conversación abierta hace un año y contestada
    // ayer se borraría con la respuesta de ayer dentro.
    let filas: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, contacto FROM conversacion WHERE ultimo_en < ?1")
            .bind(corte.to_rfc3339())
            .fetch_all(pool)
            .await?;

    if filas.is_empty() {
        return Ok(Resumen::default());
    }

    // Una constancia por contacto, no una por conversación: lo que se demuestra
    // es «a esta persona se le borró», y diez filas iguales solo hacen ruido.
    let mut por_contacto: std::collections::BTreeMap<String, Vec<i64>> = Default::default();
    for (id, contacto) in filas {
        por_contacto.entry(contacto).or_default().push(id);
    }

    let mut total = Resumen::default();
    for (contacto, ids) in por_contacto {
        let resumen = borrar(pool, &ids).await?;
        anotar(pool, &contacto, "retencion", resumen, ahora).await?;
        total.conversaciones += resumen.conversaciones;
        total.mensajes += resumen.mensajes;
    }
    Ok(total)
}

/// Borra las conversaciones indicadas y lo que cuelga de ellas.
///
/// El orden es el de las claves foráneas: primero lo que apunta, después lo
/// apuntado. Al revés, la base lo rechaza — y eso está bien: es la base
/// impidiendo dejar filas huérfanas que nadie volvería a mirar.
async fn borrar(pool: &SqlitePool, ids: &[i64]) -> Resultado<Resumen> {
    let mut resumen = Resumen::default();

    for id in ids {
        sqlx::query("DELETE FROM escalado WHERE conversacion_id = ?1")
            .bind(id)
            .execute(pool)
            .await?;

        let mensajes = sqlx::query("DELETE FROM mensaje WHERE conversacion_id = ?1")
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected();

        let conversaciones = sqlx::query("DELETE FROM conversacion WHERE id = ?1")
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected();

        resumen.mensajes += mensajes as i64;
        resumen.conversaciones += conversaciones as i64;
    }
    Ok(resumen)
}

/// Deja la constancia. **Sin nada del contenido.**
async fn anotar(
    pool: &SqlitePool,
    contacto: &str,
    motivo: &str,
    resumen: Resumen,
    ahora: DateTime<Utc>,
) -> Resultado<()> {
    sqlx::query(
        "INSERT INTO supresion (contacto, motivo, conversaciones, mensajes, ejecutada_en)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(contacto)
    .bind(motivo)
    .bind(resumen.conversaciones)
    .bind(resumen.mensajes)
    .bind(ahora.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Las constancias, la última primero. Es lo que se enseña cuando alguien
/// pregunta qué se hizo con sus datos.
pub async fn constancias(pool: &SqlitePool, limite: i64) -> Resultado<Vec<(String, String, i64, i64, String)>> {
    let filas = sqlx::query(
        "SELECT contacto, motivo, conversaciones, mensajes, ejecutada_en
         FROM supresion ORDER BY ejecutada_en DESC, id DESC LIMIT ?1",
    )
    .bind(limite)
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            Ok((
                f.try_get("contacto")?,
                f.try_get("motivo")?,
                f.try_get("conversaciones")?,
                f.try_get("mensajes")?,
                f.try_get("ejecutada_en")?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::conversacion::abrir_o_continuar;
    use crate::pruebas::base_de_prueba;
    use crate::{cuenta, escalado, mensaje, Boveda};
    use chasqui_dominio::Canal;
    use chasqui_dominio::atencion::Motivo;
    use chrono::Duration;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-01T12:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    async fn cuenta_de_prueba(pool: &SqlitePool) -> i64 {
        cuenta::guardar(
            pool,
            &Boveda::nueva("frase de las pruebas, larga y sin sentido"),
            Canal::WhatsApp,
            "PNID1",
            "Ventas",
            "TOKEN",
            t0(),
        )
        .await
        .unwrap()
    }

    /// Una conversación con un mensaje, en el momento que se diga.
    async fn conversacion(pool: &SqlitePool, cta: i64, contacto: &str, cuando: DateTime<Utc>) -> i64 {
        let c = abrir_o_continuar(pool, cta, contacto, cuando, 24).await.unwrap();
        mensaje::registrar_entrante(pool, c.id, &format!("wamid.{}", c.id), "hola", cuando)
            .await
            .unwrap();
        mensaje::registrar_saliente(pool, c.id, None, "buenas", cuando).await.unwrap();
        c.id
    }

    #[tokio::test]
    async fn olvidar_borra_todo_lo_de_una_persona_y_deja_constancia() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = conversacion(&pool, cta, "573001", t0()).await;
        escalado::crear(&pool, c, &Motivo::Peticion, t0()).await.unwrap();

        let r = olvidar(&pool, "573001", t0() + Duration::days(1)).await.unwrap();
        assert_eq!(r.conversaciones, 1);
        assert_eq!(r.mensajes, 2);

        let quedan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mensaje")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(quedan, 0);
        let colas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM escalado")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(colas, 0, "el escalado se quedó apuntando a una conversación borrada");

        let constancias = constancias(&pool, 10).await.unwrap();
        assert_eq!(constancias.len(), 1);
        assert_eq!(constancias[0].0, "573001");
        assert_eq!(constancias[0].1, "peticion");
    }

    #[tokio::test]
    async fn olvidar_a_uno_no_toca_a_los_demas() {
        // Directiva 22: el alcance va como parámetro. Si esto fallara, el daño
        // aparecería en la SEGUNDA vuelta, cuando falta el historial de otro.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        conversacion(&pool, cta, "573001", t0()).await;
        conversacion(&pool, cta, "573002", t0()).await;

        olvidar(&pool, "573001", t0()).await.unwrap();

        let quedan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(quedan, 1);
        let de_quien: String = sqlx::query_scalar("SELECT contacto FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(de_quien, "573002");
    }

    #[tokio::test]
    async fn purgar_borra_lo_vencido_y_conserva_lo_de_dentro_del_plazo() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let vieja = conversacion(&pool, cta, "573001", t0() - Duration::days(100)).await;
        let reciente = conversacion(&pool, cta, "573002", t0() - Duration::days(10)).await;

        let corte = chasqui_dominio::retencion::corte(t0(), 90);
        let r = purgar(&pool, corte, t0()).await.unwrap();
        assert_eq!(r.conversaciones, 1);

        let vivas: Vec<i64> = sqlx::query_scalar("SELECT id FROM conversacion")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(vivas, vec![reciente]);
        assert!(!vivas.contains(&vieja));
    }

    #[tokio::test]
    async fn purgar_sin_nada_vencido_no_borra_ni_deja_constancia() {
        // Una constancia de que no se borró nada es ruido en el sitio donde hay
        // que buscar la prueba de que sí se borró algo.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        conversacion(&pool, cta, "573001", t0()).await;

        let r = purgar(&pool, chasqui_dominio::retencion::corte(t0(), 90), t0()).await.unwrap();
        assert!(r.nada());
        assert!(constancias(&pool, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purgar_deja_una_constancia_por_persona_y_no_por_conversacion() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        // Tres conversaciones viejas del mismo contacto. EN ORDEN CRONOLÓGICO:
        // escrito al revés, cada mensaje cae dentro de la ventana de 24 h de la
        // conversación anterior —el reloj va hacia atrás— y las tres son una
        // sola. Lo cazó esta misma prueba al ponerse en rojo, y el fallo estaba
        // en cómo se montó el caso, no en `purgar`.
        for d in [140, 120, 100] {
            conversacion(&pool, cta, "573001", t0() - Duration::days(d)).await;
        }

        let r = purgar(&pool, chasqui_dominio::retencion::corte(t0(), 90), t0()).await.unwrap();
        assert_eq!(r.conversaciones, 3);

        let constancias = constancias(&pool, 10).await.unwrap();
        assert_eq!(constancias.len(), 1, "una constancia por conversación es ruido");
        assert_eq!(constancias[0].2, 3, "la constancia tiene que decir cuántas fueron");
    }

    #[tokio::test]
    async fn la_constancia_no_guarda_nada_del_contenido() {
        // Borrar y quedarse con el texto «para el registro» sería no borrar.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        mensaje::registrar_entrante(&pool, c.id, "wamid.x", "mi cédula es 1020304050", t0())
            .await
            .unwrap();

        olvidar(&pool, "573001", t0()).await.unwrap();

        let volcado: Vec<String> = sqlx::query_scalar(
            "SELECT contacto || ' ' || motivo || ' ' || ejecutada_en FROM supresion",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!volcado.join(" ").contains("1020304050"));
    }

    #[tokio::test]
    async fn olvidar_a_alguien_que_no_esta_no_es_un_error() {
        // Pasa de verdad: alguien pide que le borren y ya se le había borrado
        // por plazo. Fallar aquí obligaría a explicarle a la persona que su
        // solicitud «falló», cuando está atendida.
        let pool = base_de_prueba().await;
        let r = olvidar(&pool, "573999", t0()).await.unwrap();
        assert!(r.nada());
        assert_eq!(constancias(&pool, 10).await.unwrap().len(), 1, "la petición se atendió y consta");
    }

    #[tokio::test]
    async fn ciento_veinte_conversaciones_viejas_se_purgan_de_una_vez() {
        // Directiva 8 sobre el mecanismo que borra datos: el que más caro sale
        // si se equivoca de alcance.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        for i in 0..120 {
            conversacion(&pool, cta, &format!("5730{i:05}"), t0() - Duration::days(100 + i)).await;
        }
        // Y diez recientes que NO se pueden tocar.
        for i in 0..10 {
            conversacion(&pool, cta, &format!("5731{i:05}"), t0() - Duration::days(5)).await;
        }

        let r = purgar(&pool, chasqui_dominio::retencion::corte(t0(), 90), t0()).await.unwrap();
        assert_eq!(r.conversaciones, 120);

        let quedan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(quedan, 10, "se llevó por delante conversaciones dentro del plazo");
    }
    /// Crea un mensaje con un archivo guardado y devuelve (conversacion, medio).
    async fn con_archivo(pool: &SqlitePool, cta: i64, contacto: &str) -> (i64, i64) {
        let conv = abrir_o_continuar(pool, cta, contacto, t0(), 24).await.unwrap().id;
        mensaje::registrar_entrante(pool, conv, &format!("wamid.{contacto}"), "[una foto]", t0())
            .await
            .unwrap();
        let msg = mensaje::id_de_entrante(pool, &format!("wamid.{contacto}")).await.unwrap().unwrap();
        let medio_id =
            crate::medio::anotar(pool, msg, "image", "MEDIA-1", Some("image/jpeg"), None, t0())
                .await
                .unwrap();
        crate::medio::guardar(pool, medio_id, b"\xff\xd8\xff-esto-es-una-foto", Some("image/jpeg"))
            .await
            .unwrap();
        (conv, medio_id)
    }

    async fn cuantos_medios(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM medio").fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn olvidar_se_lleva_TAMBIEN_la_foto() {
        // LA PRUEBA QUE JUSTIFICA GUARDAR LOS BYTES EN LA BASE. Con los archivos
        // en una carpeta, este borrado dejaría la foto en el disco y la
        // supresión de la Ley 1581 sería falsa — y nadie lo notaría, porque las
        // filas sí habrían desaparecido.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        con_archivo(&pool, cta, "573001").await;
        assert_eq!(cuantos_medios(&pool).await, 1);

        olvidar(&pool, "573001", t0()).await.unwrap();

        assert_eq!(cuantos_medios(&pool).await, 0, "la foto sobrevivió a la supresión");
    }

    #[tokio::test]
    async fn purgar_por_plazo_se_lleva_TAMBIEN_las_fotos() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let (conv, _) = con_archivo(&pool, cta, "573002").await;
        // La conversación se envejece a mano: el plazo mide por `ultimo_en`.
        sqlx::query("UPDATE conversacion SET ultimo_en = ?1, abierta_en = ?1 WHERE id = ?2")
            .bind((t0() - Duration::days(200)).to_rfc3339())
            .bind(conv)
            .execute(&pool)
            .await
            .unwrap();

        purgar(&pool, chasqui_dominio::retencion::corte(t0(), 90), t0()).await.unwrap();

        assert_eq!(cuantos_medios(&pool).await, 0, "la foto sobrevivió al plazo de retención");
    }

    #[tokio::test]
    async fn el_borrado_de_una_conversacion_no_toca_la_foto_de_otra() {
        // Directiva 22: la limpieza se acota a lo que se va a regenerar. Un
        // CASCADE mal puesto se lleva de más y no se ve hasta la segunda vuelta.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        con_archivo(&pool, cta, "573001").await;
        con_archivo(&pool, cta, "573002").await;

        olvidar(&pool, "573001", t0()).await.unwrap();

        assert_eq!(cuantos_medios(&pool).await, 1, "se llevó la foto de quien no pidió nada");
    }
}
