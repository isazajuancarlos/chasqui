-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza Arenas
--
-- Retención y supresión: «un plazo decidido para cada dato con borrado
-- automático al vencer», que es lo que promete la propuesta en los TRES
-- paquetes y lo que justifica cobrar más que el freelance de al lado.
--
-- Hasta hoy el aviso de privacidad salía y las conversaciones se guardaban para
-- siempre. Eso es peor que no prometerlo: el cliente cree que cumple.

-- La PRUEBA de que se borró.
--
-- Habeas data no se cumple borrando: se cumple pudiendo demostrar que se borró,
-- el día que alguien reclama. Por eso queda la constancia — con el contacto, el
-- motivo, cuánto se borró y cuándo — y NO queda nada del contenido.
--
-- Guardar el contacto es deliberado y puede sonar contradictorio: es el dato
-- mínimo que hace útil la constancia («a ESTE señor se le borró todo el 3 de
-- agosto»). Sin él, la fila no prueba nada de nadie.
CREATE TABLE supresion (
    id             INTEGER PRIMARY KEY,
    contacto       TEXT    NOT NULL,
    -- peticion  = lo pidió la persona (derecho de supresión)
    -- retencion = venció el plazo que configuró el cliente
    motivo         TEXT    NOT NULL,
    conversaciones INTEGER NOT NULL,
    mensajes       INTEGER NOT NULL,
    ejecutada_en   TEXT    NOT NULL
);

CREATE INDEX supresion_por_fecha ON supresion (ejecutada_en);
