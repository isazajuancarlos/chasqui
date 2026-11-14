#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-Xiliux-Comercial
# SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
"""Congela lo que produce `qrencode` para que `qr.rs` se mida contra ello.

El oráculo de `siger-core` nació igual: el motor de Python produjo los
vencimientos, se congelaron y hoy el motor de Rust se compara con ellos. Aquí el
que dicta es **libqrencode**, que es la implementación que lee medio mundo.

Se corre a mano y su salida se versiona:

    python3 rust/siger-api/tests/generar_oraculo_qr.py

Dos decisiones que sostienen que esto MIDA algo:

1. **La capacidad no se copia de una tabla, se PREGUNTA.** Para cada versión se
   busca por bisección la cadena más larga que `qrencode` admite. Si tomara el
   número de la misma tabla que tiene `qr.rs`, un error en esa tabla saldría
   idéntico en los dos lados y el oráculo lo bendeciría.
2. **Se comprueba que la versión emitida es la pedida.** El lado del símbolo la
   delata: `(lado - 17) / 4`. Hace falta mirarlo y no basta con que `qrencode`
   salga con 0: `-v` es la versión **mínima**, así que con datos de más no falla
   —sube de versión en silencio—, y la bisección habría medido siempre la
   capacidad de la 40.
"""

import json
import pathlib
import subprocess

SALIDA = pathlib.Path(__file__).with_name("oraculo_qr.json")

# Alfabeto de relleno: imprimible y sin nada que un intérprete de órdenes
# interprete. No pasa por ningún shell —`subprocess` recibe una lista—, pero un
# oráculo que solo se puede regenerar con cuidado es un oráculo que no se
# regenera.
ALFABETO = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~:/?#[]@"


def relleno(largo: int, semilla: int) -> str:
    """Bytes reproducibles: un congruencial lineal, que no depende de la versión
    de Python ni del sistema."""
    x = semilla
    salida = []
    for _ in range(largo):
        x = (1103515245 * x + 12345) % (2**31)
        salida.append(ALFABETO[x % len(ALFABETO)])
    return "".join(salida)


def matriz(texto: str, version: int | None) -> list[str]:
    """La matriz de módulos, sin zona tranquila, como filas de `0`/`1`."""
    orden = ["qrencode", "-l", "M", "-8", "-m", "0", "-t", "ASCII", "-o", "-"]
    if version is not None:
        orden += ["-v", str(version)]
    salida = subprocess.run(
        orden + ["--", texto], capture_output=True, check=True
    ).stdout.decode()
    # `-t ASCII` gasta dos caracteres por módulo: «##» oscuro, «  » claro.
    filas = [f for f in salida.split("\n") if f]
    return ["".join("1" if f[i] == "#" else "0" for i in range(0, len(f), 2)) for f in filas]


def cabe(largo: int, version: int) -> bool:
    """¿Cabe en ESA versión? No vale que `qrencode` no proteste: con `-v` de
    mínimo, lo que no cabe sube de versión sin decir nada."""
    try:
        filas = matriz(relleno(largo, 1), version)
    except subprocess.CalledProcessError:
        return False
    return (len(filas) - 17) // 4 == version


def capacidad(version: int) -> int:
    """La longitud máxima que `qrencode` admite en esta versión, por bisección."""
    bajo, alto = 0, 3000
    while bajo < alto:
        medio = (bajo + alto + 1) // 2
        if cabe(medio, version):
            bajo = medio
        else:
            alto = medio - 1
    return bajo


def caso(que: str, texto: str, version: int | None = None) -> dict:
    filas = matriz(texto, version)
    emitida = (len(filas) - 17) // 4
    assert len(filas[0]) == len(filas), f"{que}: la matriz no es cuadrada"
    assert version is None or emitida == version, (
        f"{que}: se pidió la versión {version} y salió la {emitida}"
    )
    print(f"  {que}: versión {emitida}, {len(filas)}×{len(filas)}")
    return {"que": que, "texto": texto, "version": emitida, "modulos": filas}


def main() -> None:
    casos = []

    # Las cuarenta versiones, cada una con la carga MÁS LARGA que admite: es el
    # caso que ejercita su fila de la tabla de bloques hasta el último byte de
    # relleno.
    print("Las cuarenta versiones, llenas hasta el borde:")
    for version in range(1, 41):
        tope = capacidad(version)
        casos.append(caso(f"versión {version} llena ({tope} bytes)", relleno(tope, version), version))

    # Y lo que de verdad se va a emitir, más un puñado de cargas cortas que caen
    # en máscaras distintas.
    print("Cargas reales:")
    reales = [
        (
            "URI de enrolamiento",
            "otpauth://totp/SIGER:juan%40xiliux.com?secret=JBSWY3DPEHPK3PXP"
            "&issuer=SIGER&algorithm=SHA1&digits=6&period=30",
        ),
        (
            "URI con un correo largo",
            "otpauth://totp/SIGER:nombre.apellido%2Bcontrol%40entidad.gov.co"
            "?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=SIGER&algorithm=SHA1"
            "&digits=6&period=30",
        ),
        ("una letra", "A"),
        ("dígitos", "1234567890"),
        # Modo byte de verdad: acentos y una eñe ocupan dos bytes en UTF-8, y si
        # el indicador de longitud contara caracteres en vez de bytes el símbolo
        # saldría corto sin que nadie lo notara.
        ("UTF-8 multibyte", "Señor Ñandú — informe de gestión ✓"),
    ]
    for que, texto in reales:
        casos.append(caso(que, texto))

    SALIDA.write_text(json.dumps(casos, ensure_ascii=False, indent=1) + "\n")
    print(f"\n{len(casos)} casos en {SALIDA}")


if __name__ == "__main__":
    main()
