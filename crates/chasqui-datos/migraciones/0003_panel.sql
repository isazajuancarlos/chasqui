-- SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
-- Copyright (c) 2026 Juan Carlos Isaza
--
-- Quién entra al panel. Son las personas del negocio del cliente que atienden
-- lo que el bot escala: dos o tres, no un directorio.

CREATE TABLE usuario (
    id        INTEGER PRIMARY KEY,
    correo    TEXT    NOT NULL,
    nombre    TEXT    NOT NULL DEFAULT '',
    -- Argon2 (guaca::claves). Nunca la contraseña.
    hash      TEXT    NOT NULL,
    activo    INTEGER NOT NULL DEFAULT 1,
    creado_en TEXT    NOT NULL
);

-- El correo no distingue mayúsculas: quien se registró como «Ana@…» entra como
-- «ana@…». Sin esto, el mismo correo son dos cuentas y una de ellas no funciona.
CREATE UNIQUE INDEX usuario_correo ON usuario (lower(correo));

-- Los intentos de entrada, para frenar el ensayo de contraseñas.
--
-- La REGLA vive en `guaca::freno` y no aquí: lo que diverge cuando se copia no
-- es el INSERT, es el umbral y qué borra la cuenta de fallos. La app trae sus
-- filas y guaca decide.
CREATE TABLE intento_acceso (
    id      INTEGER PRIMARY KEY,
    correo  TEXT    NOT NULL,
    -- De dónde vino. Con el origen dentro de la clave, el atacante remoto se
    -- frena a sí mismo en vez de elegir a quién dejar fuera.
    origen  TEXT    NOT NULL,
    exito   INTEGER NOT NULL,
    momento TEXT    NOT NULL
);

CREATE INDEX intento_por_par ON intento_acceso (correo, origen, momento);
