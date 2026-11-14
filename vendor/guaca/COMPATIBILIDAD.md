<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# Decisiones de compatibilidad

**Cada línea de aquí dice que algo guardado dejó de leerse, o que se comprobó que
no.** Es el registro que exige `cargo run --bin vectores_fijos`: tocar un ancla
—`const *_AAAA` en `src/`, o un archivo de `src/vectores/`— sin una entrada que
la NOMBRE pone el guardián en rojo.

Por qué existe: los seis módulos persistentes llevan un vector fijo, y su prueba
se pone roja cuando el formato cambia. Pero **ese rojo se podía apagar pegando un
literal nuevo**, y entonces la rotura se volvía invisible: doce pruebas verdes y
las bóvedas de ayer sin abrir. Lo único que lo desalentaba era un comentario.
Ahora hay que declararlo donde se lee (#489, directiva 6).

El formato es libre, con dos reglas que sí importan:

1. **El nombre del ancla aparece literalmente, y como palabra entera.** Para los
   archivos de `src/vectores/` el nombre es la clave completa con su prefijo:
   `archivo:src/vectores/firma_2026.txt`.
2. **La entrada tiene que ser NUEVA.** El guardián guarda, junto a cada ancla, la
   huella de las LÍNEAS de aquí que la nombran: si el valor cambia, esas líneas
   tienen que cambiar también. Una entrada vieja que ya la nombraba no declara el
   cambio siguiente — sin esto, declarar una rotura una vez compraba una exención
   permanente, y la huella del archivo entero se renovaba con una coma.

Y lo que ninguna máquina puede comprobar, así que lo lee un humano: que la
entrada diga la VERDAD. Una línea que diga «este cambio no toca X» cuenta como
declaración de X para el guardián.

---

## 2026-08-12 · `TOKEN_2026` y `PENDIENTE_2026` — el formato del token cambió, dos veces

**Qué cambió.** `guaca::sesion` pasó de `id|caducidad` a `id|version|caducidad`,
con exigencia de exactamente tres campos y un DOMINIO de firma que entra en el
HMAC. Y ese mismo día, una segunda vez: el prefijo del dominio pasó a llevar su
**longitud** delante.

**Por qué.** Lo primero es #474: informes y medico habían añadido la revocación
por `version` cada uno a su copia, así que la de guaca era la única que no
servía. Lo segundo lo encontró la revisión independiente — sin la longitud, la
codificación `dominio‖carga` es ambigua y un token del dominio `"7"` para el
usuario 42 entraba como sesión plena del usuario **742**.

**Quién paga.** `chasqui`, y una sola vez: al subir su `rev` sus cookies abiertas
dejan de verificar. `informes` y `medico` no pagan —su formato local ya era el de
tres campos con dominio vacío—. `tunjo` no usa `sesion`.

**`PENDIENTE_2026` es un ancla NUEVA**, no una regenerada: el token intermedio
del segundo factor no tenía ninguna, y sin ella el vector del dominio vacío no
distinguía la codificación del prefijo (sus ocho bytes nulos son iguales en
little-endian, en big-endian y con la longitud detrás — comprobado con mutantes).
