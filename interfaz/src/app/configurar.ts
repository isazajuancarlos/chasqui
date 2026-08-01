// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// Lo que el cliente ajusta sin tocar código: el aviso de privacidad y los textos
// del bot, las preguntas frecuentes, las consultas a su sistema y sus cuentas.

import { Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { Api, Constancia, Consulta, Cuenta, Documento, ModeloConfig, Pregunta } from './api';

const ETIQUETAS: Record<string, string> = {
  'guion.aviso_privacidad': 'Aviso de privacidad (sale en el primer mensaje de cada conversación)',
  'guion.sin_respuesta': 'Cuando el bot no sabe',
  'guion.escalando': 'Cuando pasa con una persona',
  'guion.palabras_escalado': 'Frases con las que piden una persona (una por línea)',
  'retencion.dias':
    'Cuántos días se guardan las conversaciones (se borran solas al vencer). ' +
    'Este plazo lo decide usted: es el responsable del tratamiento.',
};

@Component({
  selector: 'app-configurar',
  imports: [FormsModule, MatButtonModule, MatCardModule, MatFormFieldModule,
            MatInputModule, MatSlideToggleModule],
  template: `
    <h2>Textos del bot</h2>
    @if (!ajustes()['retencion.dias']) {
      <p class="alerta">
        <strong>No hay plazo de retención configurado</strong>, así que las
        conversaciones se guardan indefinidamente. La Ley 1581 no da un número:
        obliga a que el dato no se conserve más de lo necesario, y ese plazo lo
        decide usted.
      </p>
    }
    @if (!ajustes()['guion.aviso_privacidad']) {
      <p class="alerta">
        Falta el aviso de privacidad. <strong>Sin él el bot no atiende</strong>, a
        propósito: tiene que nombrar a quien responde del tratamiento de datos, que
        es su empresa. Un aviso de plantilla se ve igual de bien y no cumple nada.
      </p>
    }
    @for (clave of claves(); track clave) {
      <mat-form-field appearance="outline" class="ancho">
        <mat-label>{{ etiqueta(clave) }}</mat-label>
        <textarea matInput rows="3" [ngModel]="ajustes()[clave]"
                  (ngModelChange)="editar(clave, $event)" [name]="clave"></textarea>
      </mat-form-field>
      <button mat-flat-button (click)="guardar(clave)">Guardar</button>
      <hr />
    }

    <h2>Preguntas frecuentes</h2>
    @for (p of preguntas(); track p.id) {
      <mat-card class="fila">
        <mat-card-content>
          <strong>{{ p.titulo }}</strong>
          <p>{{ p.respuesta }}</p>
          <p class="disparadores">Responde a: {{ p.disparadores.join(' · ') }}</p>
        </mat-card-content>
      </mat-card>
    }
    @if (preguntas().length === 0) { <p class="vacio">Ninguna todavía.</p> }

    <mat-card class="fila">
      <mat-card-content>
        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Título (para reconocerla; no se usa para buscar)</mat-label>
          <input matInput name="ptitulo" [(ngModel)]="nueva.titulo" />
        </mat-form-field>
        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Respuesta</mat-label>
          <textarea matInput rows="2" name="prespuesta" [(ngModel)]="nueva.respuesta"></textarea>
        </mat-form-field>
        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Frases que la disparan, una por línea</mat-label>
          <textarea matInput rows="3" name="pdisp" [(ngModel)]="nueva.disparadores"></textarea>
        </mat-form-field>
        @if (errorPregunta()) { <p class="alerta">{{ errorPregunta() }}</p> }
        <button mat-flat-button (click)="crearPregunta()">Añadir pregunta</button>
      </mat-card-content>
    </mat-card>

    <h2>Documentos</h2>
    <p class="disparadores">
      El asistente responde con esto cuando ninguna pregunta frecuente casa. Solo
      con esto: si la respuesta no está aquí, dice que no está y llama a una
      persona — no la inventa.
    </p>
    @if (!modelo()) {
      <p class="alerta">
        <strong>No hay modelo de lenguaje configurado</strong>, así que los
        documentos no se usan todavía. Se configura desde el servidor con
        <code>chasqui modelo configurar</code>: la clave es suya y no debe pasar
        por un navegador.
      </p>
    } @else {
      <p class="disparadores">
        Redacta {{ modelo()!.nombre }} en {{ modelo()!.url }}
        ({{ modelo()!.con_clave ? 'con clave' : 'sin clave' }}).
      </p>
    }
    @for (d of documentos(); track d.id) {
      <mat-card class="fila">
        <mat-card-content class="consulta">
          <div>
            <strong>{{ d.titulo }}</strong>
            <p class="disparadores">{{ d.caracteres }} caracteres</p>
          </div>
          <mat-slide-toggle [checked]="d.activo" (change)="activarDocumento(d, $event.checked)">
            Activo
          </mat-slide-toggle>
        </mat-card-content>
      </mat-card>
    }
    @if (documentos().length === 0) {
      <p class="vacio">
        Ninguno. Se cargan desde el servidor con <code>chasqui documento cargar</code>.
      </p>
    }

    <h2>Consultas a su sistema</h2>
    @for (c of consultas(); track c.id) {
      <mat-card class="fila">
        <mat-card-content class="consulta">
          <div>
            <strong>{{ c.titulo }}</strong>
            <p class="url">{{ c.url }}</p>
            <p class="disparadores">
              {{ c.con_credenciales ? 'Con credenciales' : 'Sin credenciales' }}
            </p>
          </div>
          <mat-slide-toggle [checked]="c.activa" (change)="activarConsulta(c, $event.checked)">
            Activa
          </mat-slide-toggle>
        </mat-card-content>
      </mat-card>
    }
    @if (consultas().length === 0) { <p class="vacio">Ninguna todavía.</p> }

    <h2>Constancias de borrado</h2>
    <p class="disparadores">
      Habeas data no se cumple borrando: se cumple pudiendo demostrar que se
      borró. Aquí está la prueba, sin nada del contenido.
    </p>
    @for (c of supresiones(); track $index) {
      <div class="cuenta">
        <span class="canal">{{ c.motivo === 'peticion' ? 'lo pidió' : 'venció' }}</span>
        <strong>{{ c.contacto }}</strong>
        <span class="disparadores">
          {{ c.conversaciones }} conversaciones, {{ c.mensajes }} mensajes — {{ c.ejecutada_en }}
        </span>
      </div>
    }
    @if (supresiones().length === 0) { <p class="vacio">Todavía no se ha borrado nada.</p> }

    <h2>Cuentas</h2>
    @for (c of cuentas(); track c.id) {
      <div class="cuenta">
        <span class="canal">{{ c.canal }}</span>
        <strong>{{ c.nombre || c.id_externo }}</strong>
        <span class="disparadores">{{ c.activa ? 'activa' : 'inactiva' }}</span>
      </div>
    }
    @if (cuentas().length === 0) {
      <p class="alerta">
        No hay ninguna cuenta dada de alta: los webhooks que lleguen se ignoran.
        Se añaden desde el servidor con <code>chasqui cuenta añadir</code>, porque
        el token de Meta no debe pasar por un navegador.
      </p>
    }
  `,
  styles: [`
    h2 { font-size: 1.1rem; margin: 1.5rem 0 .5rem; }
    .ancho { width: 100%; }
    .fila { margin-bottom: .5rem; }
    .consulta { display: flex; justify-content: space-between; align-items: center; gap: 1rem; }
    .url { font-family: monospace; font-size: .8rem; color: #555; margin: .2rem 0; }
    .disparadores { color: #666; font-size: .85rem; margin: .2rem 0; }
    .vacio { color: #666; }
    .alerta { background: #fff4e5; border-left: 4px solid #ed6c02; padding: .75rem 1rem; }
    .cuenta { display: flex; gap: .75rem; align-items: center; padding: .4rem 0; }
    .canal { font-size: .75rem; text-transform: uppercase; color: #666; width: 5.5rem; }
    hr { border: 0; border-top: 1px solid #eee; margin: 1rem 0; }
  `],
})
export class Configurar implements OnInit {
  private api = inject(Api);

  ajustes = signal<Record<string, string>>({});
  preguntas = signal<Pregunta[]>([]);
  consultas = signal<Consulta[]>([]);
  cuentas = signal<Cuenta[]>([]);
  supresiones = signal<Constancia[]>([]);
  documentos = signal<Documento[]>([]);
  modelo = signal<ModeloConfig | null>(null);

  nueva = { titulo: '', respuesta: '', disparadores: '' };
  errorPregunta = signal('');

  ngOnInit() {
    this.api.ajustes().subscribe((a) => this.ajustes.set(a));
    this.api.preguntas().subscribe((p) => this.preguntas.set(p));
    this.api.consultas().subscribe((c) => this.consultas.set(c));
    this.api.cuentas().subscribe((c) => this.cuentas.set(c));
    this.api.supresiones().subscribe((s) => this.supresiones.set(s));
    this.api.documentos().subscribe((d) => this.documentos.set(d));
    this.api.modelo().subscribe((m) => this.modelo.set(m));
  }

  crearPregunta() {
    const disparadores = this.nueva.disparadores
      .split('\n').map((d) => d.trim()).filter((d) => d.length > 0);

    // Se comprueba aquí Y en el servidor. Aquí para decirlo sin ir y volver;
    // allí porque el servidor no puede fiarse de que se haya comprobado aquí.
    if (disparadores.length === 0 || !this.nueva.respuesta.trim()) {
      this.errorPregunta.set('Hacen falta una respuesta y al menos una frase que la dispare.');
      return;
    }

    this.errorPregunta.set('');
    this.api.crearPregunta(this.nueva.titulo || disparadores[0], this.nueva.respuesta, disparadores)
      .subscribe(() => {
        this.nueva = { titulo: '', respuesta: '', disparadores: '' };
        this.api.preguntas().subscribe((p) => this.preguntas.set(p));
      });
  }

  // Orden fijo y no el que venga de la base: el aviso de privacidad va primero
  // porque es el que bloquea la atención.
  claves() { return Object.keys(ETIQUETAS); }
  etiqueta(clave: string) { return ETIQUETAS[clave] ?? clave; }

  editar(clave: string, valor: string) {
    this.ajustes.update((a) => ({ ...a, [clave]: valor }));
  }

  guardar(clave: string) {
    this.api.ponerAjuste(clave, this.ajustes()[clave] ?? '').subscribe();
  }

  activarDocumento(d: Documento, activo: boolean) {
    this.api.activarDocumento(d.id, activo).subscribe(() => {
      this.documentos.update((l) => l.map((x) => (x.id === d.id ? { ...x, activo } : x)));
    });
  }

  activarConsulta(c: Consulta, activa: boolean) {
    this.api.activarConsulta(c.id, activa).subscribe(() => {
      this.consultas.update((lista) =>
        lista.map((x) => (x.id === c.id ? { ...x, activa } : x)));
    });
  }
}
