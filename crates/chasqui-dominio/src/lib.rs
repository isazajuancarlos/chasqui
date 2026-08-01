// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//! # chasqui-dominio
//!
//! Los tipos y las reglas de Chasqui. **Aquí no entra ni la base de datos ni la
//! red**: lo que vive en este crate se prueba con `cargo test` sin levantar nada,
//! y por eso es donde van las decisiones que no pueden equivocarse — cuándo se
//! manda el aviso de privacidad, cuándo calla el bot, cuándo se llama a una
//! persona.
//!
//! La frontera está puesta a propósito. La lógica que decide qué se responde no
//! debería necesitar un SQLite ni un webhook de Meta para probarse; si lo
//! necesita, es que se mezcló con la infraestructura.

#![forbid(unsafe_code)]

pub mod atencion;
pub mod canal;
pub mod conocimiento;
pub mod conversacion;
pub mod guion;
pub mod mensaje;
pub mod texto;

pub use atencion::{Accion, Contexto, Motivo, atender};
pub use canal::Canal;
pub use conocimiento::{Entrada, buscar};
pub use conversacion::{Estado, VENTANA_HORAS_META, ventana_cerrada};
pub use guion::Guion;
pub use mensaje::{MensajeEntrante, MensajeSaliente};
