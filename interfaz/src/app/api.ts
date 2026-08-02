// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// El único sitio de la interfaz que sabe que existe una red.
//
// El panel es el PRIMER consumidor de la API, no un privilegiado: todo lo que
// hace aquí lo puede hacer la aplicación del cliente con un token. Si alguna
// pantalla necesitara algo que este servicio no puede pedir, es que la API está
// incompleta — y se arregla en la API, no en la pantalla.

import { HttpClient } from '@angular/common/http';
import { Injectable, inject, signal } from '@angular/core';

export interface Yo { id: number; correo: string; nombre: string; }
export interface Escalado {
  id: number; conversacion_id: number; contacto: string;
  motivo: string; detalle: string; creado_en: string;
}
export interface Conversacion {
  id: number; canal: string; contacto: string; estado: string;
  abierta_en: string; ultimo_en: string; mensajes: number; aviso_enviado: boolean;
}
/** Un archivo que mandó quien escribe. Sin los bytes: esos se piden aparte. */
export interface Medio {
  id: number;
  tipo: string;
  mime: string | null;
  nombre_archivo: string | null;
  /** guardado | pendiente | demasiado_grande | fallido */
  estado: string;
  detalle: string | null;
  tamano: number | null;
}
export interface Mensaje {
  id: number;
  direccion: string;
  texto: string;
  momento: string;
  medio?: Medio;
}
export interface Pregunta { id: number; titulo: string; respuesta: string; disparadores: string[]; }
export interface Consulta {
  id: number; titulo: string; url: string; activa: boolean; con_credenciales: boolean;
}
export interface Documento {
  id: number; titulo: string; caracteres: number; activo: boolean;
}
export interface ModeloConfig { url: string; nombre: string; con_clave: boolean; }
export interface Constancia {
  contacto: string; motivo: string; conversaciones: number;
  mensajes: number; ejecutada_en: string;
}
export interface Paso {
  clave: string; pregunta: string;
  /** `numero`, `correo`, `codigo`, o null para texto libre. */
  tipo_dato: string | null;
}
export interface Flujo {
  id: number; titulo: string; disparadores: string[];
  confirmacion: string; pasos: Paso[];
  /** Si el bot lo atiende. Viene del servidor: el interruptor del panel no
   *  adivina su estado inicial. */
  activo: boolean;
}
/**
 * Lo que se MANDA al crear un flujo, que no es lo mismo que se recibe.
 *
 * Sin `id` —lo pone la base— y sin `activo` —un flujo nace encendido, y decidir
 * eso desde el navegador sería el navegador mandando sobre una regla del
 * servidor—. Escribirlo como un `Omit` de lo que se recibe habría obligado a
 * inventar los dos campos para poder enviar.
 */
export interface FlujoNuevo {
  titulo: string; disparadores: string[]; confirmacion: string; pasos: Paso[];
}
export interface Pedido {
  id: number; titulo: string; contacto: string; conversacion_id: number;
  /** Pares [clave, valor] EN EL ORDEN de los pasos. Un pedido leído al revés no
   *  es el mismo pedido. */
  campos: [string, string][];
  creado_en: string;
}
export interface Cuenta {
  id: number; canal: string; id_externo: string; nombre: string; activa: boolean;
}

@Injectable({ providedIn: 'root' })
export class Api {
  private http = inject(HttpClient);

  /** Quién está dentro. `null` mientras no se sepa. */
  readonly yo = signal<Yo | null>(null);

  entrar(correo: string, clave: string) {
    return this.http.post<Yo>('/api/sesion', { correo, clave });
  }
  salir() { return this.http.delete<void>('/api/sesion'); }
  quienSoy() { return this.http.get<Yo>('/api/yo'); }

  escalados() { return this.http.get<Escalado[]>('/api/escalados'); }
  atenderEscalado(id: number) {
    return this.http.post<void>(`/api/escalados/${id}/atender`, {});
  }

  conversaciones() { return this.http.get<Conversacion[]>('/api/conversaciones'); }
  historial(id: number) { return this.http.get<Mensaje[]>(`/api/conversaciones/${id}`); }
  responder(id: number, texto: string) {
    return this.http.post<void>(`/api/conversaciones/${id}/mensaje`, { texto });
  }
  devolverAlBot(id: number) {
    return this.http.post<void>(`/api/conversaciones/${id}/devolver-al-bot`, {});
  }

  /** El derecho de supresión: borra TODO el historial del contacto. */
  olvidarContacto(id: number) {
    return this.http.post<{ conversaciones: number; mensajes: number }>(
      `/api/conversaciones/${id}/olvidar`, {});
  }
  supresiones() { return this.http.get<Constancia[]>('/api/supresiones'); }

  ajustes() { return this.http.get<Record<string, string>>('/api/ajustes'); }
  ponerAjuste(clave: string, valor: string) {
    return this.http.put<void>('/api/ajustes', { clave, valor });
  }

  preguntas() { return this.http.get<Pregunta[]>('/api/conocimiento'); }
  crearPregunta(titulo: string, respuesta: string, disparadores: string[]) {
    return this.http.post<{ id: number }>('/api/conocimiento', { titulo, respuesta, disparadores });
  }
  activarPregunta(id: number, activa: boolean) {
    return this.http.patch<void>(`/api/conocimiento/${id}`, { activa });
  }

  consultas() { return this.http.get<Consulta[]>('/api/consultas'); }
  activarConsulta(id: number, activa: boolean) {
    return this.http.patch<void>(`/api/consultas/${id}`, { activa });
  }

  flujos() { return this.http.get<Flujo[]>('/api/flujos'); }
  /** Crea el flujo, o reemplaza entero el que tenga el mismo título. */
  guardarFlujo(f: FlujoNuevo) {
    return this.http.post<{ id: number }>('/api/flujos', f);
  }
  activarFlujo(id: number, activa: boolean) {
    return this.http.patch<void>(`/api/flujos/${id}`, { activa });
  }
  borrarFlujo(id: number) { return this.http.delete<void>(`/api/flujos/${id}`); }

  pedidos() { return this.http.get<Pedido[]>('/api/pedidos'); }

  cuentas() { return this.http.get<Cuenta[]>('/api/cuentas'); }

  documentos() { return this.http.get<Documento[]>('/api/documentos'); }
  activarDocumento(id: number, activa: boolean) {
    return this.http.patch<void>(`/api/documentos/${id}`, { activa });
  }
  modelo() { return this.http.get<ModeloConfig | null>('/api/modelo'); }
}
