# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **Configuración general: `~/.claude/CLAUDE.md`** — reglas que no se rompen, las
> directivas numeradas y las máquinas montadas (hooks, permisos, skills). Se carga
> en toda sesión y **no se copia aquí**; este archivo lleva SOLO lo de guaca. El
> inventario está en `/mnt/data/conocimiento/MAPA.md`.
>
> **guaca se lleva desde SU carpeta**: `cd /mnt/data/guaca && claude`. Hasta este
> archivo era el último miembro de la «familia Quipu» que compartía el
> `CLAUDE.md` de decod; con el suyo propio sale igual que tunjo y que chuspa, y
> esa lista se queda vacía. Del `CLAUDE.md` de decod solo hace falta lo de «Quién
> consume Quipu» cuando se vaya a mover el requisito de `quipu`.

guaca es una **fachada delgada** de protección en reposo para aplicaciones, sobre
[Quipu](https://crates.io/crates/quipu). AGPL-3.0-or-later con licencia comercial
aparte (la política de Quipu, del cual es fachada). **Repositorio público.**

**No reimplementa criptografía.** Ese es el contrato del proyecto, no un detalle
de estilo: si un cambio aquí necesita escribir una primitiva, el cambio va en
Quipu. Lo que guaca aporta es la *decisión* —qué se ata a qué, qué se rechaza,
qué se hace con un dato roto—, que es exactamente lo que divergía cuando estaba
copiada en dos aplicaciones.

## Comandos

```bash
cargo test                      # 78 pruebas, ~3 s en debug (medido 2026-08-14)
cargo test freno::pruebas::ciento_veinte_intentos   # una sola, por ruta completa
cargo test freno::                                  # un módulo
cargo clippy --all-targets -- -D warnings           # EXACTAMENTE lo que corre el CI
cargo deny check                                    # cadena de suministro (deny.toml)
```

**`auditoria::simulacion_de_manipulaciones` sigue siendo la más cara del banco,
pero ya no domina**: sella 100 entradas y altera 16 posiciones, cada una con
firma híbrida Ed25519 + ML-DSA-87, y verificar una cadena es O(n). **Medido el
2026-08-14: 1,51 s de una corrida entera de ~3 s.** Sigue valiendo filtrar por
nombre al iterar sobre otro módulo, y sigue sin valer la pena mover nada a
`release`.

Aquí decía **«de los ~29 s, 20 s son una sola prueba»**, y era falso — lo cazó
la revisión del 2026-08-14, y de la peor manera: ese mismo día se corrigió la
línea de arriba (`61 pruebas, ~31 s` → la medida real) y **este párrafo, que es
el que la razonaba, se quedó vivo**. Directiva 24 fallando dentro del mismo
archivo y en la misma pasada. Un archivo que se lee en cada sesión afirmando un
coste que el artefacto desmiente es exactamente el modo de fallo caro que este
`CLAUDE.md` documenta en otro sitio. La causa del salto de 20 s a 1,5 s **no se
volvió a medir**; el sospechoso obvio es el `[profile.dev.package."*"]
opt-level = 3` de más abajo, que optimiza justo la dependencia que firma, pero
eso está sin comprobar y se escribe como sospecha, no como dato.

**`Cargo.lock` SÍ se versiona desde el 2026-08-04**, y no por convención sino por
una ceguera medida: sin él, el grafo de dependencias de GitHub solo veía los
**10** crates nombrados en `Cargo.toml` de un árbol de **119**, así que las
alertas de Dependabot no alcanzaban ni una primitiva criptográfica —`ml-dsa`,
`ml-kem`, `curve25519-dalek`, `ed25519-dalek`, `chacha20poly1305`, `subtle`,
`getrandom`, `quipu-nucleo`…—. Una alarma que no ve el 92% del árbol da
«sin alertas» y «sin mirar» con la misma cara. Los cinco repositorios hermanos ya
lo versionaban; guaca era la excepción.

**No afecta a los consumidores**: cargo ignora el lock de una dependencia, así
que medico, informes y tunjo siguen resolviendo el suyo. Lo que fija la versión
de guaca en ellos es su `rev` de git, no este archivo.

El precio, para que no sorprenda: el CI pasa a probar un árbol **fijado** en vez
de «lo último compatible», así que ya no cazará solo que una versión nueva de una
dependencia rompa algo. Eso lo cubre el PR semanal agrupado de Dependabot, que es
exactamente una resolución fresca puesta a correr contra las pruebas.

## Los dos tipos de módulo

La distinción que hay que tener clara antes de tocar nada: **la mitad de `src/`
envuelve a Quipu y la otra mitad no lo toca**. Un cambio en Quipu solo puede
romper la primera.

| Módulo | Qué hace | Qué de Quipu usa |
|---|---|---|
| `reposo` | cifrado autenticado de archivos y blobs (Argon2 → AEAD → padding Padmé) | `api::encode_to_blob` / `decode_from_blob` |
| `firma` | integridad y no repudio post-cuánticos; `canonico()` = JSON con claves ordenadas | `api::encode_signed` / `decode_verified`, `pqsign` |
| `custodia` | derivar una clave de un secreto de despliegue y repartirla 2-de-3; **generar la frase de una bóveda que nadie sabe** (`frase_nueva`) | `kdf`, `shamir` (feature `escrow`), `aleatorio` |
| `auditoria` | bitácora encadenada por hash y firmada | solo a través de `crate::firma` |
| `freno` | la regla que frena el ensayo de contraseñas | **nada** |
| `sesion` | token HMAC firmado `id\|version\|caducidad\|firma`, con dominio de firma | **nada** (`hmac` + `sha2`) |
| `claves` | contraseñas con Argon2 | **nada** (`argon2`) |

`lib.rs` no tiene más lógica que `dict()`, el alfabeto ASCII imprimible
(`0x21..=0x7e`) que Quipu usa por defecto. **Ese alfabeto entra en el fingerprint
del contenedor cifrado y en la verificación de firmas**: cambiarlo hace ilegible
todo lo ya guardado por cualquier consumidor. No es una constante de estilo.

## Lo que no puede cambiar sin romper datos ajenos

guaca produce artefactos que las aplicaciones **persisten**. Cuatro formatos son
compatibilidad hacia atrás, no decisiones reabiertas:

- **`dict()`** — ver arriba.
- **`firma::canonico`** — JSON con claves ordenadas y separadores compactos, el
  mismo esquema que `json.dumps(sort_keys=True, separators=(",",":"))`. Una firma
  hecha aquí verifica contra la rueda de Python de Quipu y al revés; tocar el
  serializador rompe esa equivalencia sin dar ningún error.
- **la codificación de la firma híbrida** (Ed25519 + ML-DSA-87) tal como la
  serializa `encode_signed` sobre `dict()`. No es código de guaca —vive en
  quipu— pero **el formato viaja en cada firma que un consumidor guarda**, así
  que romperlo aquí abajo hace que toda historia clínica y toda bitácora ya
  firmadas se lean como manipuladas. Lo ancla desde el 2026-08-05
  `firma::una_firma_de_2026_sigue_verificando`, con una firma emitida por
  **guaca `d786d1c` / quipu 0.10.0** y verificada por el binario de hoy.
- **`auditoria::preimagen`** — `{"cont": …, "prev": hex, "seq": n}`. Cambiarla
  invalida todas las cadenas ya selladas, que es precisamente lo que la cadena
  existe para hacer notar.
- **`sesion`** — el formato del token y el HMAC. Cambiarlos cierra la sesión de
  todo el mundo a la vez. **Se cambió a propósito el 2026-08-12** (0.7.0): ver
  abajo, «El formato del token se rompió el 2026-08-12, y fue una decisión».

Ante cualquiera de los cuatro: versión nueva y migración explícita del
consumidor, nunca un cambio en sitio.

**Y desde el 2026-08-04 los cuatro tienen VECTOR FIJO, que es lo que convierte
esa frase en un mecanismo. Desde el 2026-08-05 son SEIS**: se le añadió el suyo
a `custodia::derivar`, de donde sale la clave maestra, y —el mismo día, más
tarde— a **`firma`**, que era el último módulo persistente sin ancla. Hasta ese
día la lista era prosa: todas las pruebas
codificaban y decodificaban con el mismo binario, así que medían el códec contra
sí mismo y habrían pasado en verde con el formato cambiado. Ahora cada una compara
contra un artefacto **capturado antes y pegado como literal** — un blob cifrado,
dos comparticiones, un hash de cadena, un token—. Comprobado rompiendo el formato
en el CÓDIGO MEDIDO, no en el arnés:

| Se rompió esto | Se puso roja |
|---|---|
| `dict()`: alfabeto `0x7e` → `0x7d` | `reposo::un_blob_de_2026_sigue_descifrando` (`CodebookMismatch`) |
| `auditoria::preimagen`: clave `"cont"` → `"contenido"` | `el_hash_de_una_entrada_de_2026_no_ha_cambiado` |
| `sesion::firmar`: un byte más en el HMAC | `un_token_de_2026_sigue_verificando` |
| `sesion::verificar`: separador `\|` → `.` | `un_token_de_2026_sigue_verificando` |
| `custodia::recuperar`: base64 url-safe → estándar | `una_comparticion_de_2026_sigue_recuperando` |
| `custodia::derivar`: pepper `b""` → `b"mutante"` | `una_clave_derivada_de_2026_no_ha_cambiado` |
| `firma::canonico`: `to_vec` → `to_vec_pretty` | `el_canonico_ordena_las_claves` (ya existía) |

**El mismo mutante de `dict()` de la primera fila pone roja también
`firma::una_firma_de_2026_sigue_verificando`**, y ése es el dato que justifica el
sexto vector. No va como fila aparte a propósito: es un mutante, no dos, y
listarlo dos veces hacía leer la tabla como si hubiera ocho.

Con el alfabeto cambiado, en `firma.rs` se ponen rojas **dos** de las tres
pruebas nuevas —`una_firma_de_2026_sigue_verificando` y
`el_vector_de_firma_discrimina`, ésta por `Invalida` y no por `Alterada`— y las
**seis que ya existían siguen en VERDE**: `firma_valida_verifica`,
`un_cambio_posterior_se_detecta_como_alterada`, `otra_clave_no_verifica` y las
tres de entrada inválida. Las seis firman y verifican con el MISMO binario, así
que un cambio de formato les sale coherente consigo mismo. Llevaban desde
siempre pareciendo cobertura de `firma`, y no cubrían lo único que no se puede
rehacer: una firma ya emitida.

Dos matices del sexto que conviene no leer de más, los dos medidos en la
revisión de guaca#18:

- **`el_vector_de_firma_discrimina` no es un control independiente**, es un
  segundo ancla del mismo formato: cae con el mismo mutante. El control de
  verdad de la pareja es otro —romper la comparación del payload en
  `firma::verificar`—, y ahí sí: `una_firma_de_2026_sigue_verificando` queda
  VERDE y solo cae su pareja. Que es exactamente para lo que existe.
- **El vector ancla la CODIFICACIÓN, no la fuerza del esquema.** Si una versión
  futura de quipu dejara de verificar la mitad ML-DSA y comprobara solo Ed25519,
  la firma congelada seguiría verificando y las tres pruebas seguirían verdes
  mientras la propiedad post-cuántica desaparece en silencio. Eso no lo puede ver
  ninguna prueba de guaca —es del lado de quipu—, y por eso queda escrito: su
  verde **no** dice «la firma híbrida sigue siendo híbrida».

De ese último mutante salió el dato que justifica el quinto vector, y vale
guardarlo: con el pepper cambiado, `una_clave_derivada_de_2026_no_ha_cambiado`
se puso ROJA mientras `la_derivacion_es_determinista` y
`distinto_salt_da_distinta_clave` **siguieron en verde**. Las dos comparaban
`derivar()` contra `derivar()` en el mismo binario, que es exactamente el
defecto que esta lista existe para cerrar — y llevaban desde siempre pareciendo
cobertura.

**Y desde el 2026-08-12 eso lo IMPIDE una máquina, no un comentario** (#489).
`cargo run --bin vectores_fijos` calcula el sha256 del VALOR de cada ancla leyendo
el ÁRBOL, lo compara con `vectores.lock`, y si algo cambió exige su entrada en
`COMPATIBILIDAD.md` **nombrando el ancla como palabra entera**. No prohíbe romper
la compatibilidad: obliga a declararlo donde se lee. Hoy vigila **15 anclas** —
las 12 constantes y los 3 archivos de `src/vectores/`—. Va como paso del CI, que
es de donde `verde_local.py` lee qué correr.

**Y `--escribir` EXIGE LO MISMO QUE COMPROBAR.** Fue el hallazgo de la SEGUNDA
revisión y era peor que los cinco primeros juntos: la detección había mejorado y
**la exigencia había desaparecido**. Romper el KDF, pegar el literal nuevo,
`--escribir`, y el CI entero en verde sin haber tocado `COMPATIBILIDAD.md`. La
directiva 6 pide una máquina que IMPIDA; aquello solo recordaba.

**Y la declaración tiene que ser NUEVA, no solo nombrar el ancla.**
`COMPATIBILIDAD.md` es acumulativo, así que con «¿aparece el nombre?» un ancla
declarada una vez quedaba fuera de vigilancia **para siempre** — medido: ya había
2 de 15 así, y la lista solo crecía. Por eso el lock guarda DOS huellas: la del
valor y la de la declaración con que se fijó. Si el valor cambia, la declaración
tiene que cambiar también. Es el «declarado en ESTE cambio» que la versión con
`git diff` tenía gratis, recuperado sin git.

**Mide el VALOR y no el diff, y esa forma la impuso una revisión que desarmó la
primera versión en cinco puntos.** Aquella leía `git diff` buscando líneas con
`const *_AAAA`, y de ahí salían los cinco: `BLOB_2026` ocupa CUATRO líneas y solo
la primera lleva `const`, así que editar el último tramo del hex rompía el blob
—`un_blob_de_2026` en rojo con «lo cifrado el 2026-08-04 dejó de abrirse»— y el
guardián decía «ninguno tocado»; `find("const ")` casaba dentro de un literal, y
**el guardián se denunciaba a sí mismo** exigiendo declarar una constante
inventada de sus propias pruebas; en un `push` a `main` el diff contra
`origin/main` está VACÍO, o sea que el commit de fusión no se medía nunca; un
clon superficial no tiene `origin/main` y el paso quedaba en rojo permanente sin
medir; y `const BUFFER_4096` se reportaba como ancla porque la regla era «cuatro
dígitos» y no «un año». Leyendo el árbol, los cinco desaparecen.

**Y una lección de método que hay que decir**, porque es la directiva 31 en su
forma más cara: este archivo llegó a afirmar «la rama que solo añade el guardián
sale VERDE», y era **falso** — se midió antes de escribirle las pruebas y no se
volvió a correr. La rama salía roja contra su propio guardián. Un veredicto sin
artefacto caduca sin avisar.

Lo demás que la segunda revisión dejó cerrado: la clave del lock lleva la RUTA
—dos anclas homónimas en módulos distintos colapsaban en una y la perdida dejaba
de vigilarse **sin que el conteo llegara a cero**—; un ancla cuyo valor es un
`include_*` se mide por el ARCHIVO, no por el texto que lo declara; y un
comentario de cola ya no se traga la constante siguiente ni entra en el valor.

**Y una frase que hubo que quitar de aquí, porque era falsa y es la SEGUNDA vez
en el mismo trabajo**: decía que ese seguimiento «alcanza también a
`tests/oraculo_qr.json`». **No lo alcanza.** El oráculo se carga con un
`include_str!` dentro del CUERPO de una función (`src/qr.rs:809`), no desde la
línea de un `const`, así que no entra en el lock y nadie lo vigila — y es justo el
artefacto cuya documentación dice «se regenera con un script y no se toca a
mano». Cubrirlo cuesta un `const ORACULO_2026: &str = include_str!(…)` usado por
la prueba; mientras no exista, la frase no se escribe. Escribir en el archivo que
se lee en cada sesión una afirmación que el artefacto no cumple es el modo de
fallo más caro que tiene este sistema (directiva 31). **Y la exclusión de `src/bin/` se
RETIRÓ**: medida, daba las mismas 15 anclas — era un segundo mecanismo
conteniendo lo que el parser anclado ya resolvía (directiva 1).

**LO QUE SIGUE ABIERTO, y se escribe en vez de callarse** (tercera revisión):

- **CERRADO: `main` ya se mide de extremo a extremo** (`tests/guardian_e2e.rs`, 8
  pruebas). Arranca el binario REAL contra un árbol temporal —vía la variable
  `GUACA_VECTORES_RAIZ`, que reapunta la raíz solo para el arnés y por defecto
  cae al árbol real (directiva 20)— y comprueba el CÓDIGO DE SALIDA y el mensaje,
  que es como le habla el CI (directiva 28). Cubre el verde, tres rojos por vías
  distintas (ancla cambiada sin declarar, declarar OTRA ancla —el caso
  `TOKEN_2026_BIS`—, y un archivo de `src/vectores/` manipulado), los dos NO
  CONCLUYENTE (sin lock y cero anclas, exit 2, no verde), y la doble negativa de
  `--escribir` (exige lock y exige declarar). Verificado que discrimina: mutando
  la negativa de `--escribir` —el «botón verde» de más arriba—,
  `escribir_sin_lock_exige_crear` se pone roja; restaurado, 78 + 8 en verde,
  clippy limpio.
- **Dos anclas homónimas en dos `mod` del MISMO archivo siguen colapsando**: la
  clave es por archivo, no por módulo. Alcanzable — `src/sesion.rs` ya tiene dos.
- **`#[ignore]` sobre la prueba de un ancla sigue invisible**: el literal no
  cambia, y lo que se quita es lo único que le da sentido. Son dos propiedades
  distintas y el lock solo tiene campo para una (directiva 32). El criterio del
  revisor, que comparto: se cierra con una entrada `prueba:<ruta>::<fn>` que
  hashee el CUERPO de la prueba —mismo lector, mismo lock, ninguna máquina
  nueva—, acotada a las pruebas que nombran un ancla. Va DESPUÉS de esto, no
  antes: una prueba castrada exige que alguien actúe a propósito.
- **Una ruta absoluta en un `include_*`** hace que el veredicto dependa del
  estado de la máquina. Rebuscado, y se cierra rechazándolas con `NO CONCLUYENTE`.

Sus casos rojos, contra roturas REALES del código medido: cambiar el último tramo
del literal multilínea de `BLOB_2026` (con su positivo de control: la prueba del
blob se pone roja de verdad), tocar un archivo de `src/vectores/`, y declarar
`TOKEN_2026_BIS` cuando lo que cambió es `TOKEN_2026` —que la versión anterior
daba por declarado con un `contains`—. Y el verde: el árbol tal como está. Sale
con **2** —no con 0— cuando no puede mirar: «sano» y «no se pudo comprobar» tienen
que verse distinto (directiva 30).

**Si una de esas se pone roja, NO se regenera el literal.** Roja significa que lo
guardado hasta hoy dejó de leerse: archivos cifrados que no abren, bitácoras que
se acusan solas de manipuladas, sesiones cerradas de golpe. Es una rotura de
compatibilidad que se decide y se comunica, no un vector que envejeció.

La única que sí puede envejecer legítimamente es la de `sesion`, porque su token
lleva caducidad —2226—; por eso lleva al lado
`el_vector_de_sesion_no_esta_a_punto_de_caducar`, que avisa con diez años de
margen y dice cómo regenerarlo.

### El formato del token se rompió el 2026-08-12, y fue una decisión

El vector de `sesion` **se regeneró** ese día, que es justo lo que el párrafo de
arriba prohíbe hacer a la ligera. Queda escrito porque la regla no era «no se
regenera nunca» sino «no se regenera sin decidirlo y comunicarlo»:

- **Qué cambió.** `id|caducidad` → `id|version|caducidad`, exactamente tres
  campos, y las firmas pasan por un **dominio** que entra en el HMAC y no viaja
  en el token (`emitir_en`/`verificar_en`; el dominio vacío es la sesión normal).
- **Por qué.** Medido el 2026-08-04: informes y medico le habían añadido la
  revocación por `version` cada uno a SU copia, con parsers distintos, y el
  dominio de firma lo necesitaba el token «pendiente de segundo factor» de #212.
  La copia de guaca era **la única que no servía**, así que la deduplicación iba
  al revés de lo obvio: sube lo que las apps aprendieron. Es la tarea #474.
- **Quién paga cookies vivas AL RELEASE (2026-08-12): nadie** —el único consumidor
  de `sesion` era `chasqui`, con la forma de dos campos, y no se enteraba hasta
  subir su `rev` (al subirlo añade `, 0` en dos llamadas y sus cookies se cierran
  UNA vez)—. **Pero quien MIGRE su copia a este módulo SÍ paga (corregido
  2026-08-31).** Aquí decía que informes y medico «ya emitían tres campos con
  dominio vacío, o sea que bajar al módulo común les es transparente»: es FALSO.
  La misma 0.7.0 añadió el prefijo de longitud del dominio, que cambia el HMAC
  hasta con dominio vacío, así que una copia SIN prefijo no es compatible por
  emitir tres campos. **Medido al migrar informes (2026-08-31): cierre de sesión
  único** (opción A, elegida sobre una ventana de doble verificación). medico
  pagará igual —su copia es del mismo origen—, y se mide aparte.
- **Y se rompió una segunda vez el mismo día**, al cerrar lo que encontró la
  revisión independiente: el prefijo del dominio ahora lleva su **longitud**
  delante. Sin ella, `mac.update(dominio)` + `mac.update(carga)` firma una
  concatenación ambigua — medido sobre el código: un token del dominio `"7"` para
  el usuario 42, con un `7` pegado delante, entraba como **sesión plena del
  usuario 742**. Con los dominios de hoy no se llegaba ahí, pero eso lo sostenía
  la ORTOGRAFÍA de una cadena y no el tipo, y `emitir_en` acepta cualquier `&str`.
  Se hizo AHORA, en la misma ventana en la que el formato ya se rompe y nadie ha
  pagado todavía; después habría costado un segundo cierre de sesión general.
- **La otra mitad de esa revisión**: `ahora()` devolvía `0` ante un reloj
  imposible y eso fallaba ABIERTO en la verificación —`caduca_en <= 0` es falso
  para todo token, así que revivían hasta los caducados—. Hoy devuelve `Option` y
  la decisión vive en `vigente(caduca_en, ahora)`, separada **para poder pasarle
  un reloj imposible en una prueba**: sin separarla, el arreglo no lo cubría nada
  y el mutante lo demostró.
- **El vector viejo** era `42|8093089565|UzfoE9NNYV8QtwbmbJzRmQb8iUupLFCkabPtQlslguA`
  (emitido con 0.4.0). El nuevo se generó con el propio binario —el test
  `generar_el_vector_fijo`, marcado `#[ignore]`— y no a mano, que es la trampa
  que la directiva 33 señala: un control fabricado por fuera coincide con la
  medición en un valor que la librería nunca produjo.
- **Sus dos casos rojos, comprobados con mutante en el código medido**: relajar
  los tres campos a `split_once` deja entrar un token de la 0.6 y pone roja
  `un_token_del_formato_viejo_no_entra`; sacar el dominio del HMAC pone rojas las
  dos de separación de dominios. Con las dos mutaciones el resto siguió verde.

## El contrato con las aplicaciones que la consumen

**guaca no tiene estado, ni base de datos, ni framework web, y no los va a
tener.** La app trae sus filas y guaca decide; la app guarda las columnas que
guaca devuelve. Es lo que permite que la misma regla valga en medico e informes
sin arrastrarles un `sqlx` ni un `axum`.

- `freno::evaluar(intentos, ahora)` recibe las marcas de tiempo **sin parsear**,
  a propósito: qué hacer con una fecha ilegible es una decisión de seguridad, y
  si la app parsea antes de llamar, se queda ella con la decisión —que es justo
  lo que este módulo existe para evitar—. Las dos decisiones van en la dirección
  conservadora: un *fallo* ilegible cuenta como recién ocurrido, un *acierto*
  ilegible **no** borra la cuenta de fallos.
- `auditoria::sellar_con` / `verificar_con` toman el firmante como *closure*: el
  esquema de firma lo pone el llamante. Así tunjo le inyecta su firma **triple**
  (Ed25519 + ML-DSA-87 + SLH-DSA) para la cadena de custodia mientras medico e
  informes usan el atajo `sellar`/`verificar` con la doble por defecto. Al tocar
  la cadena, el camino genérico es el que hay que mantener; los atajos son
  azúcar sobre él.
- `freno::corte_retencion` existe para que el `DELETE` de la app y el conteo de
  `evaluar` usen **el mismo borde**. Si aparece un segundo borde en algún sitio,
  es un defecto.

**`UMBRAL`, `BASE_SEGUNDOS`, `TOPE_SEGUNDOS` y `RETENCION_HORAS` son la política
de seguridad de dos productos.** Cambiar cualquiera es un cambio de tres
repositorios (aquí, medico e informes), y sus pruebas de ruta afirman el mismo
borde que las de aquí: tienen que decir lo mismo. El comentario de `espera_de`
guarda el porqué de `<` y no `<=` — lo cazó la prueba de la ruta, no las de este
crate.

Lo que el freno **no** resuelve, y no debe aparentar que sí: el relleno de
credenciales desde muchas máquinas contra muchas cuentas. Contra eso va el
segundo factor. Este módulo es su prerrequisito, no su sustituto.

## Lo que no se usa no entra en el árbol (features, 2026-08-12)

`dosfactores` y `qr` van detrás de features, con `default = []`:

```toml
guaca = { git = "…", rev = "…", version = "0.7", features = ["dosfactores"] }
```

**El motivo es una medición, no una preferencia.** Al subir `dosfactores` aquí,
`tunjo` —que solo usa `auditoria`— se llevó CUATRO crates que no llama:
`totp-rs`, `sha1`, `base32` y `constant_time_eq`. En un universo que se retiró de
los registros públicos precisamente por la cadena de suministro, un consumidor no
puede pagar la superficie de una regla que no usa.

Comprobado con su positivo de control (`cargo tree -e normal`): **0** de los
cuatro por defecto, **los 4** con `--features dosfactores`. Y el banco corre en
las dos configuraciones — **78 pruebas por defecto, 95 con `--all-features`**—,
que es lo que impide que un `#[cfg]` mal puesto deje un módulo sin probar y en
verde. Los dos únicos `cfg(feature)` del crate son POSITIVOS y no hay ni un
`cfg(not(…))`, así que la unión de las dos configuraciones cubre el crate entero.

**El límite, que hay que decir porque la promesa no la sostiene este archivo**:
cargo UNIFICA las features dentro de un mismo grafo de build. Si un consumidor
tiene otro crate que pida `guaca` con `dosfactores`, los cuatro vuelven a entrar
también para él. La medición de arriba vale para el grafo de hoy; quien la
sostiene es el `Cargo.toml` de cada consumidor, no el de guaca.

`qr` va aparte porque no cuesta dependencias (945 líneas sin ninguna), pero solo
tiene sentido con el enrolamiento: `dosfactores` lo enciende.

**Quién tiene que pedirlo**: medico e informes (usan las dos). tunjo y chasqui,
nada — y por eso adelgazan.

## Cómo llega guaca a sus consumidores

**No está publicada en crates.io** (verificado 2026-08-04 contra el índice
sparse: `NoSuchKey`). Los tres consumidores la fijan por **`rev` de git**, nunca
por tag —un tag es mutable, y moverlo a un commit malicioso nos lo traería al
construir: es el ataque Atomic Arch—:

**Esto se MIDE, no se recuerda.** Cada consumidor se lleva desde su propia
instancia, así que su `rev` y sus llamadas cambian sin que aquí ocurra nada — el
2026-08-04 los dos primeros pasaron de `2ffe134` a `d786d1c` mientras se escribía
esta misma sección. Los dos comandos, probados tal cual:

```bash
grep -rho 'guaca::[a-z_]*' /mnt/data/medico /mnt/data/informes /mnt/data/tunjo \
  --include=*.rs | sort | uniq -c
grep -rn 'guaca = ' /mnt/data/medico/Cargo.toml \
  /mnt/data/informes/rust/siger-api/Cargo.toml /mnt/data/tunjo/Cargo.toml
```

Lo estable es **qué módulo usa cada uno** —eso solo cambia cuando alguien decide
usar otro—; el número de llamadas se mueve cada tarde y no vale la pena fijarlo
aquí.

| Consumidor | Qué módulos usa | `rev` (medido 2026-08-12) |
|---|---|---|
| `/mnt/data/medico` | `firma`, `freno`, `auditoria`, `reposo`, **`dosfactores`**, **`qr`** | `161a687` (= `v0.6.0`) |
| `/mnt/data/informes` | `freno`, `reposo` (cifrado de entregas) | `161a687` (= `v0.6.0`) |
| `/mnt/data/tunjo` | `auditoria` con firmante triple propio | `161a687` (= `v0.6.0`) |

**LOS TRES EN EL MISMO `rev`, y se restauró el mismo día que se rompió**
(2026-08-12). medico subió al traerse el segundo factor, e informes y tunjo
detrás con su propio verde: 262 y 54 pruebas, clippy limpio en los dos, y
`cargo deny` con `licenses ok, sources ok` en tunjo.

**Ese `sources ok` era CIERTO al escribirse y dejó de serlo dos días después**, y
conviene dejarlo dicho porque el 2026-08-16 pareció una mentira del registro y no
lo era. El 2026-08-14 el commit «quipu sale del ESPEJO, no de crates.io» mudó la
fuente de quipu en cinco repositorios de una pasada y no tocó `allow-git` en
ninguno, así que a partir de ese día `sources` estaba en ROJO aquí, en tunjo, en
chasqui, en medico y en informes. Se arregló el 08-16. La diferencia entre un
documento que MIENTE y uno que CADUCÓ no da igual: el primero se corrige, el
segundo se fecha — y este estaba fechado, que es lo que permitió resolverlo con
`git log -S` en vez de a ojo. Para ellos el cambio es
ADITIVO —entran `dosfactores` y `qr` y nadie los llama—, y aun así se hace: tres
consumidores en dos versiones de una librería de SEGURIDAD es exactamente la
divergencia que dio origen a este crate.

**Informes tiene además una razón propia**: `dosfactores` salió de ahí, y su
`segundofactor.rs` sigue con la copia local. Podarla para que consuma la de guaca
no se podía ni empezar mientras su `rev` no trajera el módulo; ahora sí.

**Ojo al subir**: `version` va junto al `rev`. Los tres fijaban `version = "0.4"`,
y cargo comprueba las dos, así que mover solo el `rev` rompe el build en vez de
traer nada.

**`qr` es de la 0.6.0 y viene con `dosfactores`**, del mismo sitio y por el mismo
motivo: es el dibujo del `otpauth://` del enrolamiento, y separarlos habría
dejado media pantalla en cada repositorio. **No convierte a guaca en una
librería de imágenes**: son 945 líneas sin una sola dependencia —la alternativa
era la *feature* `qr` de `totp-rs`, que arrastra `image` entero para producir un
rectángulo en blanco y negro—. Su oráculo son las matrices de `libqrencode` para
las CUARENTA versiones, y al mudarlo se regeneró contra el `qrencode` de la
máquina: idéntico byte a byte, así que el fixture no llegó rancio.

**`dosfactores` es de la versión 0.5.0**, y llegó por el camino que la directiva 9
describe: **nació en informes** el 2026-08-02, escrito a propósito para mudarse
—sin `sqlx`, sin configuración, sin tipos de la aplicación—, y **subió aquí el
2026-08-12**, el día que medico necesitó lo mismo (#212). No antes: lo que sube es
lo que YA se repitió en un segundo proyecto, nunca por previsión.

Y lo que impide no es hipotético. La revisión de seguridad de informes encontró
que el contador antirreplay se quemaba en una sentencia aparte, y **tres
peticiones simultáneas con el mismo código abrían tres sesiones** (medido:
`[200,200,200]`). Con dos copias, ese arreglo habría vivido en una sola.

**Los consumidores tienen que acabar en el MISMO `rev`** —«un solo árbol de guaca
en todo el universo»—. Medido el 2026-08-12: medico, informes y tunjo fijan los
tres `161a687` (= `v0.6.0`), y **`chasqui` es el cuarto y va detrás**, en
`23395f3`. Cada uno se mueve desde su instancia y con su propio verde.

**Y la 0.7.0 los ordena de nuevo, con una diferencia**: para medico, informes y
tunjo el salto es aditivo salvo en `sesion`, que **rompe** (tres campos y dominio
de firma). informes y medico SÍ pagan cookies al migrar —el prefijo de longitud
del dominio cambia el HMAC aunque el formato de texto ya fuera de tres campos,
medido al migrar informes el 2026-08-31—: borran su copia, llaman aquí, y hay un
cierre de sesión único (informes ya lo hizo; medico se mide aparte). chasqui paga
`, 0` en dos llamadas y una rotación de cookies. tunjo no usa `sesion`.

**`claves` y `sesion` YA se usan — `informes` migró.** Hasta el 2026-08-04 esto
decía «no los usa NADIE: cero llamadas en los tres», y describía el trabajo que
faltaba: `informes` hasheaba por su cuenta con `SaltString::generate(&mut
OsRng)`. Recontado el **2026-08-05**, la absorción ocurrió: `guaca::claves::hashear`
se llama desde `siger-api/src/main.rs:107` y `importar.rs:474`, y el censo por
módulo en los consumidores queda así:

| Módulo | Llamadas | |
|---|---|---|
| `claves` | 31 | |
| `firma` | 28 | |
| `freno` | 27 | lo comparten `informes` y `medico` |
| `reposo` | 21 | |
| `sesion` | 11 | |
| `auditoria` | 4 | |
| `custodia` | **0** | nadie todavía — ver abajo |

**`custodia` es el único que sigue sin un solo cliente**, y esa es ahora la
frase que describe trabajo pendiente. No sobra: es el respaldo de la clave de
firma partida con Shamir, y quien debería consumirlo aún guarda la clave en el
entorno. **Su vector fijo se capturó igualmente, y a propósito antes del primer
cliente**: hacerlo hoy cuesta una prueba; hacerlo después obliga a elegir entre
romperle la bóveda a alguien o congelar un KDF sin saber cuál era.

**Y el 2026-08-14 se le preparó el primero: el producto comercial de custodia**
(tarea #214, fase 2 del orden que fijó Juan). Le faltaba la pieza que ese
producto no puede escribir por su cuenta sin reimplementar criptografía —
`frase_nueva`, la frase de 256 bits que **nadie llega a saber**, en el mismo
alfabeto portable que las comparticiones. Es la diferencia entre un respaldo y
una custodia: `derivar` sale de un secreto que una persona recuerda, así que esa
persona puede abrir sola; `frase_nueva` no la sabe nadie desde que se reparte.

Con ella se cerró el hueco de verdad, que **no era Shamir**: la matemática ya
estaba probada a fondo (110 secretos, 3-de-5, umbral que discrimina). Lo que no
probaba nadie es que la frase RECUPERADA vuelva a **abrir** el depósito, y eso
cruza `custodia` y `reposo` — una frase que se reconstruye perfecta pero ya no
descifra deja las dos mitades en verde y la bóveda cerrada para siempre, y se
descubre el día de una apertura. Lo sujeta ahora
`tres_de_cinco_abren_el_deposito_y_dos_no`, con los tres casos que decide un
cliente: umbral justo con **las diez** combinaciones, **sobrándole custodios**
(4 y 5 comparticiones presentes), y **uno menos del umbral** (*m−1*).

Sus mutantes, contra el código medido y en ejes distintos:

| Se rompió esto en `frase_nueva` | Se puso roja |
|---|---|
| no llamar al azar (deja los 32 ceros) | `dos_frases_nuevas_no_coinciden` **y** `las_comparticiones_de_una_boveda_no_abren_otra` |
| recortar la frase a 128 bits | `la_frase_trae_los_256_bits_que_promete`, y **solo** ésa |
| alfabeto base64 estándar sin relleno | `la_frase_esta_en_el_alfabeto_de_las_comparticiones` **8/8 corridas**; `la_frase_trae_los_256_bits…` solo **4/8** |

La primera fila trae el hallazgo que valida la pareja: con la frase constante,
**las comparticiones de una bóveda abren cualquier otra**. Esa consecuencia no
la veía ninguna prueba de tamaño ni de aleatoriedad.

La tercera fila es la razón de existir de la prueba del alfabeto, y salió de la
revisión: la de tamaño **no** caza ese cambio de forma fiable —solo se pone roja
si la frase contiene `+` o `/`, teóricamente el 73,6 % de las veces, medido 4 de
8—, y una prueba que falla tres de cada cuatro veces se lee como inestable y
acaba silenciada. La nueva afirma la FORMA (longitud exacta y alfabeto) y cae
siempre.

### Lo que la revisión del 2026-08-14 corrigió, y conviene no repetir

La primera versión de todo esto pasó los dos revisores sin una vulnerabilidad,
y aun así traía **dos afirmaciones que iban más lejos que su prueba**. Las dos
vivían en el `//!` del módulo y en este archivo, no dentro de la prueba, que es
donde nadie las contrasta:

- «con el umbral justo abre con **cualquier combinación**» estaba sostenido por
  **3 de los 10** tríos. Hoy se recorren los diez y se comprueba que son diez.
- el caso rotulado «faltando un custodio (*n−1* = 4 presentes)» entregaba
  `&partes[..4]` a una variable que luego rebanaba `[1..4]`: **tres**
  comparticiones. Era un cuarto trío con otro nombre, y su comentario describía
  una medición que no ocurría. Peor: **en todo el crate `recuperar` no se
  llamaba nunca con más comparticiones que el umbral**, así que la ceremonia
  real —vienen cuatro y entregan cuatro— no la recorría ninguna prueba mientras
  el módulo la vendía como la razón de ser del umbral.

**La lección de método**: una nota de honestidad construida sobre «¿qué mutante
me pone roja?» está incompleta. Falta el segundo eje, **«¿cubro el ALCANCE de lo
que se afirma sobre mí?»**, y es el que decide si la documentación miente. La
mía cubría el primero y no el segundo.

**Lo que hay que leer con cuidado, y está escrito encima de la prueba**:
`tres_de_cinco_abren_el_deposito_y_dos_no` es un guardián de COMPOSICIÓN y
**quedó VERDE con los mutantes de aleatoriedad y de tamaño** — cifra y descifra
con la misma frase, así que cualquier cambio coherente consigo mismo la deja
pasar. Su trabajo es ponerse roja el día que `reposo` y `custodia` dejen de
entenderse.

### `Frase`: dos observaciones convertidas en imposibles

`frase_nueva` no devuelve `Zeroizing<String>` sino un tipo propio, y las dos
razones salieron medidas de la revisión (directiva 6: máquina que impide >
directiva que recordar). Las dos se comprobaron **por compilación fallida**, que
es un caso rojo más fuerte que una prueba:

| Lo que antes compilaba | Hoy |
|---|---|
| `frase_nueva().unwrap_or_default()` → frase VACÍA, con la que `reposo::cifrar` sella tan contento (quipu no tiene piso de longitud) y el llamante solo se estrella un paso después, en `repartir`, **con la bóveda ya escrita** | `the trait bound Frase: Default is not satisfied` |
| `println!("{}", frase)` y `tracing::info!(?frase)` sacaban los 256 bits en claro — `Zeroizing` protege la RAM, no la salida | `Frase doesn't implement Display`; `Debug` escribe `Frase(<oculta>)`, sujeto por `el_debug_de_la_frase_no_la_imprime` |

Es la misma decisión que ya endurecía a `CustodiaError` —no decir de más—
aplicada al valor de ÉXITO, que es la llave entera. La asimetría la señaló el
revisor y tenía razón.

### Con sobres de SOBRA manda la POSICIÓN, y eso muerde en una ceremonia

Lo encontró la tercera revisión, y es la única observación del día que cambia lo
que un producto tiene que programar. `quipu::shamir::combine` interpola con las
**primeras** `umbral` comparticiones —`let usadas = &shares[..needed]`, leído en
la fuente, no deducido— y descarta el resto **sin mirarlas**.

Consecuencia en un 3-de-5 con los cinco sobres presentes y **uno malo**, medida
en `con_sobrantes_una_mala_decide_por_donde_esta` (con su control positivo: los
cinco buenos abren):

| Dónde quedó el sobre malo | |
|---|---|
| al final | **abre** — ni se mira |
| delante | **no abre**, aunque haya cuatro buenos en la mesa |

Es el MISMO conjunto de sobres en distinto orden, con resultados opuestos. Quien
monte el producto de custodia encima no puede recoger los sobres en el orden en
que llegan los custodios y dar la bóveda por cerrada al primer `Err`: hay que
**probar subconjuntos**. `recuperar` no lo hace y no aparenta hacerlo, pero lo
natural al programarlo es suponer que sí.

Y el endurecimiento de `Frase` cubre GENERAR, no RECUPERAR: `recuperar` devuelve
`Zeroizing<Vec<u8>>`, que **sí** deriva `Debug`, así que un
`tracing::info!(?recuperada)` imprime la frase como lista de bytes. Es API
general de Shamir y devolver bytes es correcto; simplemente ahí no hay tipo que
lo impida, y es justo la ruta donde alguien tiene la frase entera en memoria.

### El límite de la promesa comercial, que no es un defecto y hay que decir

**«Ni el operador puede abrir solo» vale hasta la PRIMERA APERTURA.** Shamir no
es cómputo multiparte: `recuperar` junta las *m* comparticiones en un solo
proceso, así que quien ejecuta la ceremonia tiene la frase entera en memoria y
nada le impide retenerla. Desde ahí abre solo, para siempre, sin convocar a
nadie — y las comparticiones de los custodios **siguen pareciendo válidas**, o
sea que nadie se entera.

No se arregla con criptografía; se arregla con **procedimiento: resellar tras
cada apertura** (frase nueva, reparto nuevo, comparticiones viejas destruidas).
Quien monte el producto de custodia encima tiene que implementarlo **y decirlo
en el material de venta**: sin eso, la frase describe la bóveda en reposo y no
la que ya se abrió una vez. Está escrito también en el `//!` del módulo, que es
donde lo leerá quien programe.

Y una asimetría que conviene conocer: la frase sale protegida y las
comparticiones que devuelve `repartir` son `String` en claro que nadie borra.
Es correcto —tienen que sobrevivir a la llamada— pero cualesquiera *m* de ellas
**son** la llave.

Queda un rastro de la etapa anterior que conviene mirar cuando se toque
`informes`: su `Cargo.toml` sigue declarando `argon2` y `password-hash`, y hay
un `SaltString::generate` en `pruebas_cronograma.rs:34`. Si ya solo sirven a las
pruebas, son dependencias que se pueden bajar a `dev-dependencies` — pero eso se
mide en `informes`, no se supone desde aquí.

**Por qué el tag `v0.4.0` está en `d786d1c` y no en `2ffe134`**, que es donde se
subió el número y donde apuntaban los dos consumidores hasta esa tarde: entre
esos dos commits `src/`, `Cargo.toml` y `deny.toml` son **idénticos** —la
librería es la misma— y lo único que añade `d786d1c` es el archivo `LICENSE`.
Etiquetar `2ffe134` habría dejado una referencia pública y permanente a un árbol
sin licencia, que en un repositorio público GitHub lee como «todos los derechos
reservados». Un tag no se mueve; el sitio importa.

Mientras apuntaron a `2ffe134`, medico e informes **vendorizaban un árbol sin
`LICENSE`** — mismo código, árbol incompleto. Los dos subieron a `d786d1c` el
mismo 2026-08-04, cada uno desde su instancia. Se deja escrito porque es el modo
de fallo, no el incidente: **un `rev` a pelo no dice de qué versión sale**, y
cualquier commit vale como ancla aunque le falte medio árbol. Fijar el commit del
tag lo cierra.

**Publicar aquí no actualiza a nadie.** Subir la versión obliga a editar el `rev`
en cada consumidor que deba recibirlo, y eso es la directiva 24 en la misma
pasada — el hook de aislamiento preguntará al escribir en `medico`, `informes` o
`tunjo`, y hay que aceptarlo, no dejarlo anotado.

**Hacia abajo, guaca va SIEMPRE a la versión actual de Quipu** — hoy
`quipu = "0.11"`, resuelto a `0.11.0` en el `Cargo.lock`. Es una regla de la
familia, decidida por Juan el **2026-08-05**, y sustituye a la decisión que
estuvo escrita aquí unas horas de quedarse en la 0.10.

**El motivo es que Quipu es NUESTRA.** La directiva 35 —agotar la línea actual,
el salto mayor con motivo y nunca por inercia— existe para no perseguir el
*major* de un tercero entre visitas al cliente. Aquí las dos puntas son de casa:
quien corta la release decide también cuándo la toman los derivados. Y quedarse
atrás tiene un coste que ir al día no tiene: mientras guaca y tunjo no vayan
parejos, el binario de tunjo acaba con **dos copias de la pila cripto** —lo suyo
y lo que arrastra guaca por el `rev`—, medido el 2026-08-05.

**LA PUERTA, que es lo que separa la regla de la inercia: se sube, y si un
VECTOR FIJO se pone rojo, la subida SE DETIENE y vuelve a ser una decisión.** Un
rojo ahí no es una prueba quisquillosa: dice que lo ya cifrado dejó de abrirse.
No se regenera el literal — eso convierte una rotura de compatibilidad en un
verde. Los **seis** vectores están arriba, con la tabla de mutantes que demuestra
que cada uno discrimina.

**Y hoy nada lo IMPIDE: solo está escrito.** No hay hook, ni `CODEOWNERS`, ni
comprobación de CI que detecte que un archivo de `src/vectores/` cambió en un PR
— es la directiva 6 sin cumplir (máquina que impide > prueba que falla >
directiva que recordar) justo donde más caro sale, porque el arreglo tentador
—regenerar el literal y ver verde— es *más barato* que el correcto. Y el paso a
archivos lo empeora un poco frente a los cinco literales en línea: un literal
alterado salta a la vista en el diff de `firma.rs`; un `firma_2026.txt` de 5883
caracteres reescrito entero es una sola línea `+`/`-` que nadie va a leer. Está
registrado como tarea `#489`, y la guardia barata que sí discriminaría es un job
que falle si el diff toca `src/vectores/` y el PR no lleva etiqueta explícita de
rotura de compatibilidad.

**El orden con tunjo son TRES pasos, no dos**, y el tercero es el que se olvida:

1. guaca sube `quipu` y entra en `main`.
2. tunjo sube `quipu` **y** mueve el `rev` de guaca a ese commit, **en el mismo
   commit suyo**. Si solo sube `quipu`, sigue arrastrando la versión vieja por
   el `rev` viejo.
3. Control de que salió bien, antes de dar nada por bueno:
   `grep -c '^name = "quipu"$' Cargo.lock` tiene que dar **1** en tunjo.

`chuspa` no entra en la regla: compila por `path = "../quipu"`, así que ya va
contra el árbol por definición.

**La compatibilidad con 0.11 SÍ está verificada** (2026-08-05). Aquí decía que no
lo estaba y que no se podía suponer; se midió, y el resultado es que guaca
compila y pasa **56/56** contra `quipu 0.11.0` en un worktree aislado, con los
cinco vectores fijos dentro — incluido `un_blob_de_2026_sigue_descifrando`, que
es lo que prueba que un blob escrito con 0.10 se abre con 0.11.

Lo que faltaba era `custodia::derivar`, y **ya no falta**: `derive_master_key`
con `KdfParams::default()` da la MISMA maestra byte a byte bajo 0.10 y 0.11
—medido con un arnés pareado, un binario por versión, comprobando en los dos
`Cargo.lock` que cada uno enlazó la suya—. Y para que eso deje de ser una
medición de una tarde, la clave quedó pegada como **vector fijo** en
`una_clave_derivada_de_2026_no_ha_cambiado`. Con el mutante puesto en el código
medido, esa prueba se pone roja y las dos que ya existían siguen en VERDE: son
ciegas al KDF porque comparan `derivar()` contra `derivar()` en el mismo
binario.

Lo que sigue SIN verificar es el otro lado del acoplamiento, y es tarea de
tunjo: `src/clave.rs` tampoco tiene vector fijo. Que un `.clave` de 0.10 abra
con 0.11 está comprobado cruzando los binarios reales, pero nada lo sujeta.

**Los tags son anotados, uno por versión, y `v0.4.0` se puso el 2026-08-04** —
faltaba desde que se subió el número—. Los cuatro, comprobados en `origin`:
`v0.1.0`→`40fb4b4`, `v0.2.0`→`d3d7ab1`, `v0.3.0`→`9d26cce`, `v0.4.0`→`d786d1c`.
El mensaje sigue la forma de los anteriores: `guaca X.Y.Z — <qué trae>`.

Un tag **no dispara el CI** (el workflow solo escucha `push` a `main` y
`pull_request`), así que un tag por sí solo no prueba nada. Lo que ancla el
veredicto es que su commit tenga su propia corrida verde: `d786d1c` la tiene.

## El árbol está VENDORIZADO desde el 2026-08-14

**Decisión de Juan:** «no podemos depender de herramientas online hasta saber que
todo es seguro, y eso va a tardar». El protocolo solo-local sacó el CÓDIGO de los
registros; esto saca la **construcción**. 122 crates en `vendor/`, 54 MB, con
`.cargo/config.toml` sustituyendo crates.io por ese directorio y **rutas
relativas** (cargo las escribe absolutas, y eso ata el repo a esta máquina).

**El positivo de control NO es `--offline`** —solo prueba que no salió a la red,
pudiendo tirar de `~/.cargo`—. Es la caché VACÍA:

```bash
CARGO_HOME=$(mktemp -d) cargo test --all-features --offline
```

Medido: **95 pruebas en verde**, el mismo número que antes, y la caché nueva
queda en **68 KB**. Clippy y `vectores_fijos` también en verde sin red.

**Y aquí arregla de raíz un defecto que rompía a los consumidores NUEVOS.** guaca
declaraba `quipu = "0.11"` contra crates.io y esas versiones están *yanked* por
decisión propia: cargo se niega a elegir una yankeada en una resolución NUEVA, así
que **ningún repositorio nuevo que dependiera de guaca compilaba**. Lo descubrió
`chunsua` el 2026-08-14, cuatro días después y por casualidad — los cuatro
consumidores existentes no se enteraron porque su `Cargo.lock` ya fijaba la
versión y resuelven sin tocar el índice. La cadena llevaba días rota con todo en
verde. Vendorizado, cargo ni consulta el índice ni mira el yank.

**Que esto no cambia lo que se compila no es una opinión**: `Cargo.lock` no se
toca y cada crate lleva su `.cargo-checksum.json`, que cargo verifica contra el
hash publicado — un byte distinto para el build con `the listed checksum of …
has changed`. **Pero la verificación es PEREZOSA**: salta al compilar ese crate,
no en cada build. Con el `target/` caliente, un árbol ya manipulado puede pasar
inadvertido (medido: 0,07 s y ni una palabra). Un clon nuevo o `cargo clean -p`
lo cazan siempre.

**Lo que NO compra**: `cargo deny check advisories` sigue consultando su base de
avisos por su cuenta, que es online. Congelar el árbol no dice si le sale un CVE
— y por eso los tres avisos de `yanked` de nuestros propios crates siguen
saliendo. Están nombrados en `deny.toml` para que un cuarto salte a la vista.

## CI y merge

Dos jobs en `.github/workflows/ci.yml`: `test + clippy` y `cargo-deny`. Se
dispara en `push` **solo a `main`** y en `pull_request` a `main` — cada commit se
prueba una vez, no dos, porque una rama con PR abierto disparaba los dos eventos.
**Consecuencia deliberada: una rama empujada sin PR no tiene CI.** El flujo es
rama → PR → merge.

guaca es público, así que sus minutos de Actions son gratis: la suspensión de
`push`/`merge` por cupo agotado que describe la configuración general afecta a
los repos privados, no a este.

**Las tres piezas de esto son distintas y se confunden:** `dependabot.yml`
programa las actualizaciones **rutinarias**; las **alertas** de Dependabot y las
**correcciones automáticas** son ajustes del repositorio, no archivos, y se
encendieron el 2026-08-04 (estaban las dos apagadas). Se comprueban así, y el
segundo comando es el bueno porque responde con cuerpo en vez de con un código:

```bash
gh api repos/isazajuancarlos/guaca/vulnerability-alerts -i | head -1  # 204=on, 404=off
gh api repos/isazajuancarlos/guaca/automated-security-fixes           # {"enabled":true,...}
gh api repos/isazajuancarlos/guaca/dependency-graph/sbom \
  --jq '[.sbom.packages[]|select(.name|test("/")|not)]|length'        # crates que VE
```

Ese último es el que importa: si devuelve 10 y no ~119, el lock no está en la
rama por defecto y la alarma está ciega otra vez.

**Las actualizaciones las avisa `dependabot.yml`**, semanal, agrupando lo
compatible en un solo PR y dejando FUERA los saltos de línea mayor — cada uno de
esos es una decisión con motivo (directiva 35), y hay cinco esperando: `quipu`
0.11 (deliberado: cambia `Options.codebook_id`), `argon2` 0.6 (todavía RC, así
que no), `password-hash` 0.6 (bloqueado por argon2 0.5), `hmac` 0.13 y `sha2`
0.11 (la generación nueva de RustCrypto, que desincronizaría con el `sha2` que
trae Quipu). Ninguno es de seguridad — medido el 2026-08-04 con `cargo deny check
advisories` contra una base de 1169 avisos actualizada ese día.

**`password-hash` parece una dependencia muerta y no lo es.** `claves.rs` no la
importa —usa el reexport `argon2::password_hash::…`— y `cargo tree` la enseña
colgando de argon2, así que se lee como una línea que sobra. Lo que hace es
habilitar `OsRng`: `password-hash/getrandom` → `rand_core/getrandom` → `pub use
os::OsRng`, y argon2 0.5 no activa ninguna de las dos. Quitarla mata el build con
`E0432: unresolved import argon2::password_hash::rand_core::OsRng` — comprobado
quitándola de verdad, no razonado. El motivo va escrito en la propia línea del
`Cargo.toml`, que es donde alguien iría a borrarla.

**Antes de mergear a `main`: `/security-review` desde ESTA carpeta** (directiva
25 — guaca está en la lista de repos sensibles). La skill construye su diff con
el `cwd`; lanzada desde otro repo da un diff vacío en silencio.

**Salvo que el diff sea inerte** —prosa y comentarios, nada más—, exención que se
acotó el 2026-08-04 precisamente aquí: la revisión corrió cuatro veces en una
tarde y tres fueron sobre diffs sin una línea ejecutable. El comando que lo
decide está en la directiva 25; se salta **diciéndolo**, nunca en silencio. Ojo
con la tentación de ampliar la exención a «configuración declarativa»: un
`dependabot.yml` que gana `target-branch` apaga las alertas de CVE sin ejecutar
nada, y un workflow puede cambiar `permissions:` sin añadir un solo `run:`.

## Estilo del código

Todo en español: módulos, funciones, tipos, variantes de enum y nombres de
prueba. El submódulo de pruebas se llama `mod pruebas`, no `mod tests`.

**Los comentarios explican la decisión, no la mecánica**, y en particular lo que
se rechazó: por qué no se bloquea la cuenta, por qué el techo son cinco minutos,
por qué el contenido va anidado en la preimagen, por qué un `Result` se colapsa a
`Option`. Un comentario que repite lo que hace la línea siguiente sobra; uno que
guarda el argumento de un descarte es lo que impide que alguien lo reabra dentro
de un año. Al cambiar la decisión, se cambia el comentario en la misma edición.

**Cada salvaguarda nace con su pareja de pruebas** (directiva 33): la que TIENE
que ver y la que TIENE que no ver. Los ejemplos vivos son
`sesion::cambiar_el_usuario_invalida_la_firma` («sin ella, un verificador que
siempre devuelve `Some` pasaría la anterior»), el par
`entrar_bien_borra_la_cuenta_de_fallos` /
`los_fallos_posteriores_al_acierto_si_cuentan`, y el par de marcas ilegibles del
freno. Una prueba nueva sin su reverso no está terminada.

Los errores **no dicen de más**: `custodia` no revela qué compartición estaba
corrupta, `sesion::verificar` devuelve un solo `None` para firma mala, formato
raro y caducidad, y `claves::verificar` trata un hash ilegible como «no
coincide». Eso también es la decisión, no descuido.
