<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# guaca

**Protección en reposo para aplicaciones, sobre [Quipu](https://crates.io/crates/quipu).**

_Una guaca es un tesoro enterrado y protegido — aquí, los datos en reposo._

Una fachada delgada (no reimplementa criptografía):

- **`reposo`** — cifrado autenticado de archivos y blobs sensibles (KDF Argon2 +
  AEAD + padding que oculta la longitud). Para imágenes clínicas, entregas,
  cualquier archivo que no deba leerse desde el disco.
- **`firma`** — integridad y no repudio post-cuánticos (Ed25519 + ML-DSA-87) de un
  registro. La firma se rompe si el registro cambia; distingue «alterado» de
  «firma inválida».
- **`auditoria`** — bitácora append-only **encadenada por hash y firmada**: cada
  entrada apunta a la anterior, así que borrar o reordenar rompe la cadena, no
  solo alterar. El primitivo a prueba de manipulación para una bitácora de acceso
  (Ley 1581 / OWASP A09).
- **`custodia`** — **derivar** una clave de un secreto de despliegue (Argon2) y
  **repartirla** para respaldo con Shamir (2-de-3, como el seed del OPRF): ningún
  sitio único guarda la clave entera. Para respaldo, no para firma rutinaria.

- **`freno`** — la regla que frena el ensayo de contraseñas: tras cinco fallos
  seguidos del mismo origen contra el mismo correo, el siguiente intento espera, y
  la espera se dobla hasta un techo de cinco minutos. **No bloquea la cuenta**, a
  propósito: un bloqueo convierte cinco peticiones de un desconocido en un usuario
  legítimo fuera del sistema. No toca la base — la app trae sus filas y `evaluar`
  decide, porque lo que diverge al copiar es la regla, no el `INSERT`.

También trae `claves` (Argon2 para contraseñas), `sesion` (token HMAC firmado
`id|version|caducidad`, con revocación selectiva y dominio de firma) y
`dosfactores` con su `qr` — los primitivos que medico e informes tenían
duplicados.

## Por qué existe

Medida real: dos aplicaciones (medico e informes) copiaban la misma sesión y el
mismo endurecimiento de secreto, y **las copias ya habían divergido** —una mejora
de seguridad se quedó en un solo lado—. El código de seguridad duplicado es código
de seguridad divergente. En `guaca` vive una vez, se audita una vez, se arregla
una vez.

## Uso

```rust
use guaca::{reposo, firma};

// Cifrar un archivo en reposo
let blob = reposo::cifrar(bytes, &frase_del_despliegue);
let claro = reposo::descifrar(&blob, &frase_del_despliegue)?;

// Firmar un registro y verificarlo después
let (clave_publica, clave_privada) = firma::generar_claves();
let datos = firma::canonico(&campos);      // JSON canónico determinista
let sig = firma::firmar(&datos, &clave_privada).unwrap();
assert_eq!(firma::verificar(&datos, &sig, &clave_publica), firma::Verificacion::Valida);
```

```rust
use guaca::{auditoria, custodia};
use guaca::auditoria::GENESIS;

// Bitácora encadenada: sellar la entrada n sobre el hash de la n-1.
let sello = auditoria::sellar(0, GENESIS, &campos, &clave_privada).unwrap();
// … guardar (secuencia, hash_anterior, contenido, sello.hash, sello.firma) …
// y más tarde, releer la tabla y comprobar la cadena entera:
assert_eq!(auditoria::verificar(&entradas, &clave_publica), auditoria::Auditoria::Intacta);

// Custodia: respaldar una clave repartida 2-de-3.
let partes = custodia::repartir(clave_privada.as_bytes(), 2, 3).unwrap();
let recuperada = custodia::recuperar(&partes[..2].to_vec()).unwrap();
```

**La frase de cifrado y la clave privada de firma NUNCA se guardan junto al dato.**
Vienen de un secreto de despliegue (o de un HSM / repartidas con Shamir para
respaldo).

## Licencia

AGPL-3.0-or-later, con licencia comercial disponible (misma política que Quipu).
