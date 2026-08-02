#!/usr/bin/env python3
# SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
# Copyright (c) 2026 Juan Carlos Isaza Arenas
"""Convierte los volcados de GeoNames en el archivo que se empotra en el binario.

POR QUÉ EXISTE: el panel enseña «Amagá, Antioquia, Colombia» al lado de unas
coordenadas, y eso NO puede resolverse preguntándole a un servidor de mapas: la
consulta llevaría dentro la ubicación exacta de un cliente del cliente, que es
justo lo que la frase «las conversaciones no salen de su servidor» promete que
no pasa. Así que se resuelve en local, con esta tabla dentro del ejecutable.

NIVEL ELEGIDO, medido el 2026-08-02 y no supuesto: con `cities15000` un punto
real de Amagá se etiquetaba como «Caldas» —otro municipio, a 10 km—; con
`cities5000` sale Amagá a 1,6 km; y `cities1000` cuesta 2,5 veces más filas para
el mismo resultado.

FORMATO (little-endian, sin dependencias para leerlo):
    magia   b"CHGEO1"
    u32     n ciudades
    u32     largo de la tabla de textos
    [textos]  todos los nombres pegados, UTF-8
    n x registro de 16 bytes: f32 lat, f32 lon, u32 desplazamiento del nombre,
                              u8 largo del nombre, u16 índice de división, u8 índice de país
    u32     n divisiones + tabla de (desplazamiento,largo)
    u32     n países  + tabla de (desplazamiento,largo)

NOMBRES EN CASTELLANO: GeoNames escribe en inglés —«Mexico City», «Japan»—, y el
panel está en castellano. Los países salen de `iso-codes`, que trae las
traducciones oficiales de la ISO y está en el propio sistema; las ciudades y las
divisiones, de `alternateNamesV2` filtrado por `es`, prefiriendo el nombre
marcado como preferente. Lo que no tenga nombre en castellano se queda con el
suyo, que es mejor que traducirlo a ojo.

LICENCIA DE LOS DATOS: GeoNames, CC BY 4.0. La atribución es obligatoria y va en
NOTICE; el archivo derivado la arrastra.

DEPENDENCIAS DE ESTE GUION (no del binario): los volcados de GeoNames y el
paquete `iso-codes` instalado. Se ejecuta a mano cuando haya que regenerar la
tabla; el archivo resultante SÍ viaja en el repositorio, para que compilar no
necesite red.
"""
import struct, sys, unicodedata
from pathlib import Path

ENTRADA = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
SALIDA = Path(sys.argv[2] if len(sys.argv) > 2 else "ciudades.bin")

# --- nombres en castellano ---------------------------------------------------

# Los identificadores de GeoNames cuyo nombre castellano interesa: el de cada
# ciudad que se va a guardar y el de cada división. Se recogen ANTES de leer el
# archivo grande para poder descartarlo casi entero mientras se lee.
ids_interesantes = set()
for linea in (ENTRADA / "cities5000.txt").read_text(encoding="utf-8").splitlines():
    c = linea.split("\t")
    if len(c) >= 15:
        ids_interesantes.add(c[0])

divisiones = {}
ids_division = {}
for linea in (ENTRADA / "admin1.txt").read_text(encoding="utf-8").splitlines():
    c = linea.split("\t")
    if len(c) >= 4:
        divisiones[c[0]] = c[1]
        ids_division[c[3]] = c[0]
        ids_interesantes.add(c[3])

# alternateNamesV2: geonameid, iso-idioma, nombre, esPreferente, esCorto...
castellano = {}
preferentes = set()
ruta_alt = ENTRADA / "alternateNamesV2.txt"
if ruta_alt.exists():
    with ruta_alt.open(encoding="utf-8") as f:
        for linea in f:
            c = linea.rstrip("\n").split("\t")
            if len(c) < 4 or c[2] != "es" or c[1] not in ids_interesantes:
                continue
            gid, nombre = c[1], c[3]
            preferente = len(c) > 4 and c[4] == "1"
            # El preferente manda; si no lo hay, vale el primero que aparezca.
            if preferente:
                castellano[gid] = nombre
                preferentes.add(gid)
            elif gid not in preferentes and gid not in castellano:
                castellano[gid] = nombre
    for gid, clave in ids_division.items():
        if gid in castellano:
            divisiones[clave] = castellano[gid]
else:
    print("AVISO: sin alternateNamesV2.txt — las ciudades saldrán con su nombre de GeoNames")

# Países: iso-codes, que trae la traducción oficial y está en el sistema.
import json, gettext

paises = {}
try:
    tabla_iso = json.load(open("/usr/share/iso-codes/json/iso_3166-1.json"))["3166-1"]
    tr = gettext.translation("iso_3166-1", "/usr/share/locale", languages=["es"]).gettext
    paises = {p["alpha_2"]: tr(p["name"]) for p in tabla_iso}
except Exception as e:
    print(f"AVISO: sin iso-codes ({e}); los países saldrán en inglés")

# Lo que iso-codes no tenga se completa con GeoNames antes que dejarlo vacío.
for linea in (ENTRADA / "countryInfo.txt").read_text(encoding="utf-8").splitlines():
    if linea.startswith("#") or not linea.strip():
        continue
    c = linea.split("\t")
    if len(c) >= 5:
        paises.setdefault(c[0], c[4])

textos = bytearray()
indice = {}


def guardar(s: str) -> tuple[int, int]:
    """Devuelve (desplazamiento, largo) del texto, sin repetirlo."""
    if s in indice:
        return indice[s]
    b = s.encode("utf-8")
    if len(b) > 255:
        b = b[:255]
    par = (len(textos), len(b))
    textos.extend(b)
    indice[s] = par
    return par


tabla_div, idx_div = [], {}
tabla_pais, idx_pais = [], {}
ciudades = []

for linea in (ENTRADA / "cities5000.txt").read_text(encoding="utf-8").splitlines():
    c = linea.split("\t")
    if len(c) < 15:
        continue
    nombre = castellano.get(c[0], c[1])
    lat, lon, pais_cc, admin1 = float(c[4]), float(c[5]), c[8], c[10]
    clave_div = f"{pais_cc}.{admin1}"
    nombre_div = divisiones.get(clave_div, "")
    nombre_pais = paises.get(pais_cc, pais_cc)

    if nombre_div not in idx_div:
        idx_div[nombre_div] = len(tabla_div)
        tabla_div.append(guardar(nombre_div))
    if nombre_pais not in idx_pais:
        idx_pais[nombre_pais] = len(tabla_pais)
        tabla_pais.append(guardar(nombre_pais))

    off, largo = guardar(nombre)
    ciudades.append((lat, lon, off, largo, idx_div[nombre_div], idx_pais[nombre_pais]))

assert len(tabla_pais) < 256, "más de 255 países: el índice de país necesita más de un byte"

with SALIDA.open("wb") as f:
    f.write(b"CHGEO1")
    f.write(struct.pack("<II", len(ciudades), len(textos)))
    f.write(textos)
    for lat, lon, off, largo, d, p in ciudades:
        f.write(struct.pack("<ffIBHB", lat, lon, off, largo, d, p))
    f.write(struct.pack("<I", len(tabla_div)))
    for off, largo in tabla_div:
        f.write(struct.pack("<IB", off, largo))
    f.write(struct.pack("<I", len(tabla_pais)))
    for off, largo in tabla_pais:
        f.write(struct.pack("<IB", off, largo))

print(f"{len(ciudades)} ciudades, {len(tabla_div)} divisiones, {len(tabla_pais)} países")
print(f"{SALIDA}: {SALIDA.stat().st_size / 1024:.0f} KB")
