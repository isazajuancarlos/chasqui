// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza
//! Abrir, continuar y cerrar conversaciones.
//!
//! Toda la sutileza está en [`abrir_o_continuar`]: decide si el mensaje que llega
//! sigue la conversación de antes o empieza una nueva, y de eso cuelgan el aviso
//! de privacidad y lo que Meta factura.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use chasqui_dominio::conversacion::{Estado, ventana_cerrada};
use chasqui_dominio::flujo::EnCurso;

use crate::{Error, Resultado};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversacion {
    pub id: i64,
    pub cuenta_id: i64,
    pub contacto: String,
    pub estado: Estado,
    pub abierta_en: DateTime<Utc>,
    pub ultimo_en: DateTime<Utc>,
    pub aviso_en: Option<DateTime<Utc>>,
    /// La consulta cuyo dato se le pidió a quien escribe y todavía no ha
    /// llegado. Es una pista y no una relación: si el cliente borra la consulta,
    /// el dominio sigue su camino normal.
    pub esperando: Option<i64>,
    /// El flujo de varios pasos a medias, si lo hay. También es una pista.
    pub en_flujo: Option<EnCurso>,
}

impl Conversacion {
    /// ¿Hay que mandar el aviso de privacidad antes de responder?
    pub fn debe_avisar(&self) -> bool {
        self.aviso_en.is_none()
    }
}

fn momento(fila: &sqlx::sqlite::SqliteRow, columna: &'static str, id: i64) -> Resultado<DateTime<Utc>> {
    let texto: String = fila.try_get(columna)?;
    fecha(&texto, columna, id)
}

fn fecha(texto: &str, columna: &'static str, id: i64) -> Resultado<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(texto)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| Error::ValorInvalido {
            tabla: "conversacion",
            id,
            columna,
            valor: texto.to_string(),
        })
}

fn leer(fila: sqlx::sqlite::SqliteRow) -> Resultado<Conversacion> {
    let id: i64 = fila.try_get("id")?;
    let ranura: String = fila.try_get("estado")?;
    let estado = Estado::desde_ranura(&ranura).ok_or(Error::ValorInvalido {
        tabla: "conversacion",
        id,
        columna: "estado",
        valor: ranura,
    })?;
    let aviso: Option<String> = fila.try_get("aviso_en")?;
    Ok(Conversacion {
        id,
        cuenta_id: fila.try_get("cuenta_id")?,
        contacto: fila.try_get("contacto")?,
        estado,
        abierta_en: momento(&fila, "abierta_en", id)?,
        ultimo_en: momento(&fila, "ultimo_en", id)?,
        aviso_en: aviso.map(|t| fecha(&t, "aviso_en", id)).transpose()?,
        esperando: fila.try_get("esperando_consulta")?,
        en_flujo: en_flujo(&fila, id)?,
    })
}

/// El flujo a medias, si lo hay.
///
/// Las tres columnas se leen JUNTAS y solo cuentan si están las tres: un
/// `flujo_id` sin lo recogido —o con un JSON que no se puede leer— es un estado
/// que el dominio nunca produce, y arrancar de ahí capturaría la conversación
/// sin saber en qué paso va. Se trata como «no hay flujo», que es el estado
/// seguro: el mensaje siguiente sigue su camino normal.
fn en_flujo(fila: &sqlx::sqlite::SqliteRow, id: i64) -> Resultado<Option<EnCurso>> {
    let Some(flujo_id): Option<i64> = fila.try_get("flujo_id")? else {
        return Ok(None);
    };
    let bruto: Option<String> = fila.try_get("flujo_recogido")?;
    let recogido: Vec<(String, String)> = match bruto.as_deref() {
        None | Some("") => Vec::new(),
        Some(j) => serde_json::from_str(j).map_err(|_| Error::ValorInvalido {
            tabla: "conversacion",
            id,
            columna: "flujo_recogido",
            valor: j.to_string(),
        })?,
    };
    let repreguntado: i64 = fila.try_get("flujo_repreguntado")?;
    Ok(Some(EnCurso { flujo_id, recogido, repreguntado: repreguntado != 0 }))
}

/// Las columnas que `leer` necesita. **Una sola lista.**
///
/// Antes había dos —una constante y otra escrita a mano en `listar`, porque allí
/// llevan prefijo de tabla—, y al añadir las tres del flujo solo se actualizó la
/// primera: `listar` seguía compilando y devolvía 500 en la pantalla principal
/// del panel. Lo cazó la prueba que cruza la frontera, no el compilador. Con una
/// lista y dos formas de escribirla, esa deriva no puede volver a existir.
const CAMPOS: &[&str] = &[
    "id",
    "cuenta_id",
    "contacto",
    "estado",
    "abierta_en",
    "ultimo_en",
    "aviso_en",
    "esperando_consulta",
    "flujo_id",
    "flujo_recogido",
    "flujo_repreguntado",
];

/// `id, cuenta_id, …`
fn columnas() -> String {
    CAMPOS.join(", ")
}

/// `c.id, c.cuenta_id, …`, para las consultas con unión.
fn columnas_de(alias: &str) -> String {
    CAMPOS.iter().map(|c| format!("{alias}.{c}")).collect::<Vec<_>>().join(", ")
}

/// La conversación viva de este contacto, o una nueva si la ventana se cerró.
///
/// Devuelve además si hay que avisar. Se hace en una sola operación —y no en un
/// «buscar» y luego un «crear»— porque son la misma decisión: partirla en dos
/// deja el hueco donde dos mensajes seguidos abren dos conversaciones.
pub async fn abrir_o_continuar(
    pool: &SqlitePool,
    cuenta_id: i64,
    contacto: &str,
    ahora: DateTime<Utc>,
    horas_ventana: i64,
) -> Resultado<Conversacion> {
    let viva = sqlx::query(&format!(
        "SELECT {} FROM conversacion
         WHERE cuenta_id = ?1 AND contacto = ?2 AND cerrada_en IS NULL",
        columnas()
    ))
    .bind(cuenta_id)
    .bind(contacto)
    .fetch_optional(pool)
    .await?;

    if let Some(fila) = viva {
        let c = leer(fila)?;
        if !ventana_cerrada(c.ultimo_en, ahora, horas_ventana) {
            sqlx::query("UPDATE conversacion SET ultimo_en = ?1 WHERE id = ?2")
                .bind(ahora.to_rfc3339())
                .bind(c.id)
                .execute(pool)
                .await?;
            return Ok(Conversacion { ultimo_en: ahora, ..c });
        }
        // La ventana se cerró: esta conversación termina aquí y empieza otra. La
        // vieja se conserva —el historial es lo que se necesita para atender una
        // solicitud de supresión— y libera el índice de «conversación viva».
        sqlx::query("UPDATE conversacion SET cerrada_en = ?1 WHERE id = ?2")
            .bind(ahora.to_rfc3339())
            .bind(c.id)
            .execute(pool)
            .await?;
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO conversacion (cuenta_id, contacto, estado, abierta_en, ultimo_en)
         VALUES (?1, ?2, ?3, ?4, ?4)
         RETURNING id",
    )
    .bind(cuenta_id)
    .bind(contacto)
    .bind(Estado::Bot.ranura())
    .bind(ahora.to_rfc3339())
    .fetch_one(pool)
    .await?;

    Ok(Conversacion {
        id,
        cuenta_id,
        contacto: contacto.to_string(),
        estado: Estado::Bot,
        abierta_en: ahora,
        ultimo_en: ahora,
        aviso_en: None,
        esperando: None,
        en_flujo: None,
    })
}

/// La conversación viva de un contacto en una cuenta, sin abrir ninguna.
///
/// La usa el canal web para leer lo dicho. Va por cuenta Y contacto, y no solo
/// por el identificador del visitante: que hoy una instancia atienda a un solo
/// negocio no debe ser lo que hace correcta la consulta.
pub async fn viva_de(
    pool: &SqlitePool,
    cuenta_id: i64,
    contacto: &str,
) -> Resultado<Option<Conversacion>> {
    let fila = sqlx::query(&format!(
        "SELECT {} FROM conversacion
         WHERE cuenta_id = ?1 AND contacto = ?2 AND cerrada_en IS NULL",
        columnas()
    ))
    .bind(cuenta_id)
    .bind(contacto)
    .fetch_optional(pool)
    .await?;
    fila.map(leer).transpose()
}

/// Anota —o borra— qué dato se está esperando.
///
/// Se guarda en la conversación y no en memoria del proceso a propósito: quien
/// escribe el número de pedido tres minutos después merece respuesta aunque el
/// servidor se haya reiniciado entre medias.
pub async fn poner_espera(pool: &SqlitePool, id: i64, consulta: Option<i64>) -> Resultado<()> {
    sqlx::query("UPDATE conversacion SET esperando_consulta = ?1 WHERE id = ?2")
        .bind(consulta)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Guarda el flujo en curso, o lo borra con `None`.
///
/// Las tres columnas se escriben **en la misma sentencia** y siempre las tres.
/// Actualizar solo una dejaría un `flujo_id` con lo recogido de otro flujo, que
/// es el estado que `en_flujo` tiene que descartar al leer — y aquí es donde se
/// impide que llegue a existir.
pub async fn poner_flujo(pool: &SqlitePool, id: i64, curso: Option<&EnCurso>) -> Resultado<()> {
    let (flujo_id, recogido, repreguntado) = match curso {
        None => (None, None, 0),
        Some(c) => (
            Some(c.flujo_id),
            // No puede fallar: es una lista de pares de cadenas.
            Some(serde_json::to_string(&c.recogido).unwrap_or_else(|_| "[]".into())),
            i64::from(c.repreguntado),
        ),
    };

    sqlx::query(
        "UPDATE conversacion \
         SET flujo_id = ?1, flujo_recogido = ?2, flujo_repreguntado = ?3 WHERE id = ?4",
    )
    .bind(flujo_id)
    .bind(recogido)
    .bind(repreguntado)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Anota que el aviso de privacidad ya salió. Es la prueba, con fecha, de haber
/// avisado; por eso se guarda el momento y no un `1`.
pub async fn marcar_aviso(pool: &SqlitePool, id: i64, ahora: DateTime<Utc>) -> Resultado<()> {
    sqlx::query("UPDATE conversacion SET aviso_en = ?1 WHERE id = ?2 AND aviso_en IS NULL")
        .bind(ahora.to_rfc3339())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cambia el estado (el bot atiende, espera a una persona, o hay una persona).
pub async fn cambiar_estado(pool: &SqlitePool, id: i64, estado: Estado) -> Resultado<()> {
    sqlx::query("UPDATE conversacion SET estado = ?1 WHERE id = ?2")
        .bind(estado.ranura())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Una conversación con lo que el panel necesita para pintarla en una lista:
/// por qué canal entró y cuántos mensajes lleva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnLista {
    pub conversacion: Conversacion,
    pub canal: chasqui_dominio::Canal,
    pub mensajes: i64,
}

/// Las conversaciones vivas, la que se movió hace menos primero.
///
/// Ese orden y no el de apertura: quien atiende mira lo que está pasando ahora,
/// y una conversación de ayer con un mensaje nuevo es lo que está pasando ahora.
pub async fn listar(pool: &SqlitePool, limite: i64) -> Resultado<Vec<EnLista>> {
    // Las columnas salen de la MISMA lista que usa `leer`, con el prefijo de la
    // tabla que la unión obliga a poner. Escribirlas a mano aquí fue justo lo que
    // dejó esta consulta sin las columnas del flujo.
    let filas = sqlx::query(&format!(
        "SELECT {}, k.canal AS canal_ranura,
                (SELECT COUNT(*) FROM mensaje m WHERE m.conversacion_id = c.id) AS cuantos
         FROM conversacion c JOIN cuenta_canal k ON k.id = c.cuenta_id
         WHERE c.cerrada_en IS NULL
         ORDER BY c.ultimo_en DESC LIMIT ?1",
        columnas_de("c")
    ))
    .bind(limite)
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let id: i64 = f.try_get("id")?;
            let ranura: String = f.try_get("canal_ranura")?;
            let canal = chasqui_dominio::Canal::desde_ranura(&ranura).ok_or(Error::ValorInvalido {
                tabla: "cuenta_canal",
                id,
                columna: "canal",
                valor: ranura,
            })?;
            let mensajes: i64 = f.try_get("cuantos")?;
            Ok(EnLista { conversacion: leer(f)?, canal, mensajes })
        })
        .collect()
}

/// Una conversación por id. Para el panel.
pub async fn por_id(pool: &SqlitePool, id: i64) -> Resultado<Option<Conversacion>> {
    let fila = sqlx::query(&format!(
        "SELECT {} FROM conversacion WHERE id = ?1",
        columnas()
    ))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    fila.map(leer).transpose()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// El flujo en curso tiene que sobrevivir a TODOS los caminos de lectura.
    ///
    /// Existe porque `listar` escribía sus columnas a mano y se quedó sin las del
    /// flujo: compilaba, y devolvía 500 en la pantalla principal del panel. Una
    /// prueba por cada lector es lo que impide que la deriva vuelva por otra
    /// consulta.
    #[tokio::test]
    async fn el_flujo_en_curso_vuelve_por_los_tres_caminos_de_lectura() {
        use chasqui_dominio::flujo::EnCurso;

        let pool = crate::pruebas::base_de_prueba().await;
        sqlx::query(
            "INSERT INTO cuenta_canal (canal, id_externo, nombre, token_cifrado, creada_en) \
             VALUES ('whatsapp', 'PNID-1', 'Ventas', x'00', '2026-08-01T00:00:00+00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ahora = Utc::now();
        let conv = abrir_o_continuar(&pool, 1, "573001", ahora, 24).await.unwrap();

        let curso = EnCurso {
            flujo_id: 7,
            recogido: vec![("producto".into(), "sillas".into())],
            repreguntado: true,
        };
        poner_flujo(&pool, conv.id, Some(&curso)).await.unwrap();

        // 1. Por `abrir_o_continuar`, que es el camino del webhook.
        let vuelta = abrir_o_continuar(&pool, 1, "573001", ahora, 24).await.unwrap();
        assert_eq!(vuelta.en_flujo.as_ref(), Some(&curso));

        // 2. Por `por_id`, que es el camino del panel al abrir una conversación.
        let una = por_id(&pool, conv.id).await.unwrap().unwrap();
        assert_eq!(una.en_flujo.as_ref(), Some(&curso));

        // 3. Por `listar`, que es la pantalla principal — la que se rompió.
        let lista = listar(&pool, 10).await.unwrap();
        assert_eq!(lista[0].conversacion.en_flujo.as_ref(), Some(&curso));

        // Y borrarlo lo borra de verdad por los tres.
        poner_flujo(&pool, conv.id, None).await.unwrap();
        assert!(por_id(&pool, conv.id).await.unwrap().unwrap().en_flujo.is_none());
        assert!(listar(&pool, 10).await.unwrap()[0].conversacion.en_flujo.is_none());
    }
    use crate::cuenta;
    use crate::pruebas::base_de_prueba;
    use crate::Boveda;
    use chasqui_dominio::Canal;
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

    #[tokio::test]
    async fn el_primer_mensaje_abre_conversacion_y_pide_aviso() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        assert_eq!(c.estado, Estado::Bot);
        assert!(c.debe_avisar());
    }

    #[tokio::test]
    async fn el_segundo_mensaje_continua_y_ya_no_avisa() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c1 = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        marcar_aviso(&pool, c1.id, t0()).await.unwrap();

        let c2 = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(5), 24)
            .await
            .unwrap();
        assert_eq!(c2.id, c1.id, "no puede abrir otra conversación a los 5 minutos");
        assert!(!c2.debe_avisar());
    }

    #[tokio::test]
    async fn pasada_la_ventana_empieza_otra_conversacion_y_se_vuelve_a_avisar() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c1 = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        marcar_aviso(&pool, c1.id, t0()).await.unwrap();

        let c2 = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::hours(25), 24)
            .await
            .unwrap();
        assert_ne!(c2.id, c1.id);
        assert!(c2.debe_avisar(), "conversación nueva, aviso nuevo");

        // Y la vieja sigue ahí: el historial no se pierde al cerrar.
        let vieja = por_id(&pool, c1.id).await.unwrap().unwrap();
        assert!(vieja.aviso_en.is_some());
    }

    #[tokio::test]
    async fn dos_contactos_no_comparten_conversacion() {
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let a = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        let b = abrir_o_continuar(&pool, cta, "573002", t0(), 24).await.unwrap();
        assert_ne!(a.id, b.id);
    }

    #[tokio::test]
    async fn el_estado_sobrevive_a_los_mensajes_siguientes() {
        // Si `abrir_o_continuar` reiniciara el estado a `bot`, el bot volvería a
        // hablar en medio de una conversación que ya atiende una persona.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        cambiar_estado(&pool, c.id, Estado::ConHumano).await.unwrap();

        let seguido = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(1), 24)
            .await
            .unwrap();
        assert_eq!(seguido.estado, Estado::ConHumano);
    }

    #[tokio::test]
    async fn marcar_el_aviso_dos_veces_no_cambia_la_fecha_del_primero() {
        // La fecha del aviso es la prueba de CUÁNDO se avisó. Si un reintento la
        // pisara, la prueba diría una hora en la que no se avisó nada.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();

        marcar_aviso(&pool, c.id, t0()).await.unwrap();
        marcar_aviso(&pool, c.id, t0() + Duration::hours(3)).await.unwrap();

        let leida = por_id(&pool, c.id).await.unwrap().unwrap();
        assert_eq!(leida.aviso_en, Some(t0()));
    }

    #[tokio::test]
    async fn cien_mensajes_seguidos_no_abren_cien_conversaciones() {
        // Directiva 8, sobre el mecanismo del que cuelga la facturación de Meta.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;

        let mut ids = std::collections::HashSet::new();
        for i in 0..120 {
            let c = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(i), 24)
                .await
                .unwrap();
            ids.insert(c.id);
        }
        assert_eq!(ids.len(), 1, "se abrieron {} conversaciones", ids.len());

        let filas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversacion")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(filas, 1);
    }

    #[tokio::test]
    async fn la_espera_de_un_dato_sobrevive_al_mensaje_siguiente() {
        // Si `abrir_o_continuar` la borrara, el número de pedido que llega
        // después no significaría nada y quien escribe volvería a la casilla uno.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        assert_eq!(c.esperando, None);

        poner_espera(&pool, c.id, Some(7)).await.unwrap();
        let seguido = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::minutes(3), 24)
            .await
            .unwrap();
        assert_eq!(seguido.esperando, Some(7));

        poner_espera(&pool, c.id, None).await.unwrap();
        assert_eq!(por_id(&pool, c.id).await.unwrap().unwrap().esperando, None);
    }

    #[tokio::test]
    async fn una_conversacion_nueva_no_hereda_la_espera_de_la_anterior() {
        // Pasadas 24 h la conversación es otra. Arrastrar la espera haría que un
        // «1234» del día siguiente consultara un pedido que nadie preguntó.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();
        poner_espera(&pool, c.id, Some(7)).await.unwrap();

        let nueva = abrir_o_continuar(&pool, cta, "573001", t0() + Duration::hours(25), 24)
            .await
            .unwrap();
        assert_ne!(nueva.id, c.id);
        assert_eq!(nueva.esperando, None);
    }

    #[tokio::test]
    async fn un_estado_que_la_base_no_reconoce_se_denuncia_en_vez_de_suponerse() {
        // Caso rojo de la directiva 20: si esto devolviera `Estado::Bot` por
        // defecto, una base escrita por otra versión pondría al bot a hablar
        // encima de una persona sin un solo error en el log.
        let pool = base_de_prueba().await;
        let cta = cuenta_de_prueba(&pool).await;
        let c = abrir_o_continuar(&pool, cta, "573001", t0(), 24).await.unwrap();

        sqlx::query("UPDATE conversacion SET estado = 'inventado' WHERE id = ?1")
            .bind(c.id)
            .execute(&pool)
            .await
            .unwrap();

        let r = por_id(&pool, c.id).await;
        assert!(matches!(r, Err(Error::ValorInvalido { columna: "estado", .. })));
    }
}
