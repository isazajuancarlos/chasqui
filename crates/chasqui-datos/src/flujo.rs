// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! Los flujos de varios pasos, en la base.
//!
//! Aquí solo se leen y se escriben: **la máquina de estados vive en
//! `chasqui_dominio::flujo`** y la decisión en `atencion`. Lo único que esta capa
//! decide es rechazar un flujo mal configurado ANTES de guardarlo — porque un
//! flujo roto no se descubre al cargarlo sino a mitad de la conversación de un
//! cliente, y para entonces el error lo ve quien no puede arreglarlo.

use chasqui_dominio::consulta::TipoDato;
use chasqui_dominio::flujo::{Flujo, Paso};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::{Error, Resultado};

/// Todos los flujos activos, con sus pasos en orden.
pub async fn activos(pool: &SqlitePool) -> Resultado<Vec<Flujo>> {
    cargar(pool, true).await
}

/// Todos, para el panel.
pub async fn todos(pool: &SqlitePool) -> Resultado<Vec<Flujo>> {
    cargar(pool, false).await
}

async fn cargar(pool: &SqlitePool, solo_activos: bool) -> Resultado<Vec<Flujo>> {
    let filas = sqlx::query(
        "SELECT id, titulo, disparadores, confirmacion FROM flujo \
         WHERE (?1 = 0 OR activo = 1) ORDER BY id",
    )
    .bind(i64::from(solo_activos))
    .fetch_all(pool)
    .await?;

    let mut flujos = Vec::with_capacity(filas.len());
    for fila in filas {
        let id: i64 = fila.get("id");
        flujos.push(Flujo {
            id,
            titulo: fila.get("titulo"),
            disparadores: fila
                .get::<String, _>("disparadores")
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
            confirmacion: fila.get("confirmacion"),
            pasos: pasos_de(pool, id).await?,
        });
    }
    Ok(flujos)
}

async fn pasos_de(pool: &SqlitePool, flujo_id: i64) -> Resultado<Vec<Paso>> {
    let filas = sqlx::query(
        "SELECT id, clave, pregunta, tipo_dato FROM flujo_paso WHERE flujo_id = ?1 ORDER BY orden",
    )
    .bind(flujo_id)
    .fetch_all(pool)
    .await?;

    filas
        .into_iter()
        .map(|f| {
            let bruto: Option<String> = f.get("tipo_dato");
            // Un tipo que el dominio no reconoce NO se degrada a texto libre: eso
            // aceptaría «unas pocas» como cantidad y el pedido llegaría al
            // negocio con un campo inservible. Se nombra la fila y el valor.
            let tipo_dato = match bruto.as_deref() {
                None | Some("") => None,
                Some(t) => Some(TipoDato::desde_ranura(t).ok_or_else(|| Error::ValorInvalido {
                    tabla: "flujo_paso",
                    id: f.get("id"),
                    columna: "tipo_dato",
                    valor: t.to_string(),
                })?),
            };
            Ok(Paso { clave: f.get("clave"), pregunta: f.get("pregunta"), tipo_dato })
        })
        .collect()
}

/// Guarda un flujo. Si ya había uno con el mismo título, lo reemplaza entero.
///
/// **Se valida antes de escribir.** Y se reemplaza entero en vez de mezclar
/// pasos: dos versiones a medias de un flujo darían dos conversaciones
/// distintas según cuándo empezara cada una.
pub async fn guardar(pool: &SqlitePool, flujo: &Flujo) -> Resultado<i64> {
    flujo.validar()?;

    let ahora = Utc::now().to_rfc3339();
    let disparadores = flujo.disparadores.join("\n");

    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM flujo WHERE titulo = ?1")
        .bind(&flujo.titulo)
        .execute(&mut *tx)
        .await?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO flujo (titulo, disparadores, confirmacion, creado_en) \
         VALUES (?1, ?2, ?3, ?4) RETURNING id",
    )
    .bind(&flujo.titulo)
    .bind(&disparadores)
    .bind(&flujo.confirmacion)
    .bind(&ahora)
    .fetch_one(&mut *tx)
    .await?;

    for (orden, paso) in flujo.pasos.iter().enumerate() {
        sqlx::query(
            "INSERT INTO flujo_paso (flujo_id, orden, clave, pregunta, tipo_dato) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(orden as i64)
        .bind(&paso.clave)
        .bind(&paso.pregunta)
        .bind(paso.tipo_dato.map(|t| t.ranura()))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(id)
}

/// Enciende o apaga un flujo desde el panel.
pub async fn activar(pool: &SqlitePool, id: i64, activo: bool) -> Resultado<bool> {
    let filas = sqlx::query("UPDATE flujo SET activo = ?2 WHERE id = ?1")
        .bind(id)
        .bind(i64::from(activo))
        .execute(pool)
        .await?
        .rows_affected();
    Ok(filas > 0)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use crate::pruebas::base_de_prueba;

    fn pedido() -> Flujo {
        Flujo {
            id: 0,
            titulo: "Toma de pedido".into(),
            disparadores: vec!["quiero hacer un pedido".into(), "hacer pedido".into()],
            pasos: vec![
                Paso { clave: "producto".into(), pregunta: "¿Qué producto?".into(), tipo_dato: None },
                Paso {
                    clave: "cantidad".into(),
                    pregunta: "¿Cuántas?".into(),
                    tipo_dato: Some(TipoDato::Numero),
                },
            ],
            confirmacion: "Anotado: {cantidad} de {producto}.".into(),
        }
    }

    #[tokio::test]
    async fn un_flujo_vuelve_de_la_base_igual_que_entro() {
        let pool = base_de_prueba().await;
        let id = guardar(&pool, &pedido()).await.unwrap();

        let flujos = activos(&pool).await.unwrap();
        assert_eq!(flujos.len(), 1);
        let f = &flujos[0];
        assert_eq!(f.id, id);
        assert_eq!(f.disparadores, vec!["quiero hacer un pedido", "hacer pedido"]);
        // El ORDEN de los pasos es la mitad del flujo: si volviera cambiado, el
        // bot preguntaría la cantidad antes que el producto.
        assert_eq!(f.pasos.len(), 2);
        assert_eq!(f.pasos[0].clave, "producto");
        assert_eq!(f.pasos[1].clave, "cantidad");
        assert_eq!(f.pasos[1].tipo_dato, Some(TipoDato::Numero));
        assert_eq!(f.pasos[0].tipo_dato, None);
    }

    #[tokio::test]
    async fn un_flujo_mal_configurado_no_llega_a_la_base() {
        // La validación va ANTES de escribir: un flujo roto no se descubre al
        // cargarlo, sino a mitad de la conversación de un cliente.
        let pool = base_de_prueba().await;
        let mut malo = pedido();
        malo.confirmacion = "Anotado {telefono}.".into();

        assert!(guardar(&pool, &malo).await.is_err());
        assert!(activos(&pool).await.unwrap().is_empty(), "se guardó un flujo inválido");
    }

    #[tokio::test]
    async fn volver_a_cargarlo_lo_reemplaza_entero_y_no_mezcla_pasos() {
        let pool = base_de_prueba().await;
        guardar(&pool, &pedido()).await.unwrap();

        let mut v2 = pedido();
        v2.pasos.pop();
        v2.confirmacion = "Anotado: {producto}.".into();
        guardar(&pool, &v2).await.unwrap();

        let flujos = activos(&pool).await.unwrap();
        assert_eq!(flujos.len(), 1, "quedaron dos versiones del mismo flujo");
        assert_eq!(flujos[0].pasos.len(), 1, "los pasos se mezclaron entre versiones");
    }

    #[tokio::test]
    async fn un_flujo_apagado_no_lo_ve_el_bot_pero_si_el_panel() {
        let pool = base_de_prueba().await;
        let id = guardar(&pool, &pedido()).await.unwrap();
        assert!(activar(&pool, id, false).await.unwrap());

        assert!(activos(&pool).await.unwrap().is_empty());
        assert_eq!(todos(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn borrar_el_flujo_se_lleva_sus_pasos() {
        let pool = base_de_prueba().await;
        let id = guardar(&pool, &pedido()).await.unwrap();
        sqlx::query("DELETE FROM flujo WHERE id = ?1").bind(id).execute(&pool).await.unwrap();

        let huerfanos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM flujo_paso")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(huerfanos, 0, "quedaron pasos sin flujo");
    }

    #[tokio::test]
    async fn un_tipo_de_dato_que_no_existe_se_denuncia_en_vez_de_pasar_por_texto_libre() {
        // Degradarlo a texto libre aceptaría «unas pocas» como cantidad, y el
        // pedido llegaría al negocio con un campo inservible sin un solo error.
        let pool = base_de_prueba().await;
        let id = guardar(&pool, &pedido()).await.unwrap();
        sqlx::query("UPDATE flujo_paso SET tipo_dato = 'fecha' WHERE flujo_id = ?1 AND orden = 1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(matches!(
            activos(&pool).await,
            Err(Error::ValorInvalido { tabla: "flujo_paso", columna: "tipo_dato", .. })
        ));
    }
}
