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
import { Api, Consulta, Cuenta, Pregunta } from './api';

const ETIQUETAS: Record<string, string> = {
  'guion.aviso_privacidad': 'Aviso de privacidad (sale en el primer mensaje de cada conversación)',
  'guion.sin_respuesta': 'Cuando el bot no sabe',
  'guion.escalando': 'Cuando pasa con una persona',
  'guion.palabras_escalado': 'Frases con las que piden una persona (una por línea)',
};

@Component({
  selector: 'app-configurar',
  imports: [FormsModule, MatButtonModule, MatCardModule, MatFormFieldModule,
            MatInputModule, MatSlideToggleModule],
  template: `
    <h2>Textos del bot</h2>
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

  ngOnInit() {
    this.api.ajustes().subscribe((a) => this.ajustes.set(a));
    this.api.preguntas().subscribe((p) => this.preguntas.set(p));
    this.api.consultas().subscribe((c) => this.consultas.set(c));
    this.api.cuentas().subscribe((c) => this.cuentas.set(c));
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

  activarConsulta(c: Consulta, activa: boolean) {
    this.api.activarConsulta(c.id, activa).subscribe(() => {
      this.consultas.update((lista) =>
        lista.map((x) => (x.id === c.id ? { ...x, activa } : x)));
    });
  }
}
