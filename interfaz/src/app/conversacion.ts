// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
//
// Una conversación, con lo que hace falta para que una PERSONA la atienda: el
// historial y un cuadro para responder. El mensaje sale por el mismo número que
// los del bot — para quien escribe hay un interlocutor, no dos.

import { Component, inject, input, OnDestroy, OnInit, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { Api, Mensaje } from './api';

@Component({
  selector: 'app-conversacion',
  imports: [DatePipe, FormsModule, MatButtonModule, MatFormFieldModule, MatInputModule],
  template: `
    <div class="barra">
      <h2>Conversación #{{ id() }}</h2>
      <button mat-button (click)="devolver()">Devolver al bot</button>
      <button mat-button class="peligro" (click)="olvidar()">
        {{ confirmando() ? '¿Seguro? Pulse otra vez' : 'Olvidar a esta persona' }}
      </button>
    </div>
    @if (olvidado()) {
      <p class="aviso">
        Borrado. {{ olvidado() }} — queda la constancia, sin nada del contenido.
      </p>
    }

    <div class="hilo">
      @for (m of mensajes(); track $index) {
        <div class="burbuja" [class.mia]="m.direccion === 'saliente'">
          <p>{{ m.texto }}</p>

          <!-- El archivo, cuando lo hay. Va DENTRO de la burbuja, en el sitio
               de la conversación en que llegó: quien atiende necesita ver la
               foto junto a lo que el cliente dijo antes y después. -->
          @if (m.medio; as med) {
            @switch (med.estado) {
              @case ('guardado') {
                @if (med.tipo === 'image' || med.tipo === 'sticker') {
                  <a [href]="'/api/medios/' + med.id" target="_blank" rel="noopener">
                    <img class="adjunto" [src]="'/api/medios/' + med.id" alt="Foto que envió el cliente" />
                  </a>
                } @else if (med.tipo === 'audio' || med.tipo === 'voice') {
                  <audio class="adjunto" controls [src]="'/api/medios/' + med.id"></audio>
                } @else if (med.tipo === 'video') {
                  <video class="adjunto" controls [src]="'/api/medios/' + med.id"></video>
                } @else {
                  <a class="archivo" [href]="'/api/medios/' + med.id" target="_blank" rel="noopener">
                    Descargar {{ med.nombre_archivo || med.tipo }}
                  </a>
                }
              }
              @case ('pendiente') {
                <p class="nota">Trayendo el archivo…</p>
              }
              @case ('demasiado_grande') {
                <p class="nota falta">
                  El archivo no se guardó porque pasa del tamaño máximo de esta instalación.
                  Pídaselo por otro medio. ({{ med.detalle }})
                </p>
              }
              @default {
                <!-- Decir que NO está y por qué. Lo peor sería no decir nada:
                     quien atiende creería que el cliente no mandó nada. -->
                <p class="nota falta">
                  No se pudo traer el archivo que envió. Pídaselo otra vez.
                  @if (med.detalle) { <span class="detalle">({{ med.detalle }})</span> }
                </p>
              }
            }
          }

          <span class="cuando">{{ m.momento | date: 'short' }}</span>
        </div>
      }
      @if (mensajes().length === 0) { <p class="vacio">Todavía no hay mensajes.</p> }
    </div>

    <form (ngSubmit)="responder()">
      <mat-form-field appearance="outline" class="ancho">
        <mat-label>Responder</mat-label>
        <textarea matInput name="texto" rows="2" [(ngModel)]="texto"></textarea>
      </mat-form-field>
      <button mat-flat-button type="submit" [disabled]="enviando() || !texto.trim()">Enviar</button>
      @if (error()) { <p class="error">{{ error() }}</p> }
    </form>
  `,
  styles: [`
    .barra { display: flex; align-items: center; gap: 1rem; }
    h2 { font-size: 1.1rem; }
    .hilo { display: flex; flex-direction: column; gap: .4rem; margin: 1rem 0;
            max-height: 60vh; overflow-y: auto; }
    .burbuja { background: #eceff1; border-radius: .6rem; padding: .5rem .75rem;
               max-width: 34rem; align-self: flex-start; }
    .burbuja.mia { background: #d7f0d7; align-self: flex-end; }
    .burbuja p { margin: 0; white-space: pre-wrap; }
    .cuando { font-size: .7rem; color: #666; }
    .adjunto { display: block; max-width: 100%; max-height: 18rem; border-radius: .4rem;
               margin: .4rem 0; }
    audio.adjunto { width: 18rem; max-height: none; }
    .archivo { display: inline-block; margin: .4rem 0; }
    .nota { margin: .4rem 0; font-size: .85rem; color: #555; }
    .nota.falta { color: #8a5300; }
    .detalle { color: #777; }
    .ancho { width: 100%; }
    .vacio { color: #666; }
    .error { color: #b3261e; }
    .peligro { color: #b3261e; }
    .aviso { background: #e8f5e9; border-left: 4px solid #2e7d32; padding: .75rem 1rem; }
  `],
})
export class ConversacionVista implements OnInit, OnDestroy {
  private api = inject(Api);
  id = input.required<number>();

  mensajes = signal<Mensaje[]>([]);
  texto = '';
  enviando = signal(false);
  error = signal('');
  // Dos pulsaciones y no un `confirm()`: el borrado es irreversible y el
  // diálogo del navegador se acepta sin leerlo.
  confirmando = signal(false);
  olvidado = signal('');
  private reloj?: ReturnType<typeof setInterval>;

  ngOnInit() {
    this.recargar();
    this.reloj = setInterval(() => this.recargar(), 5_000);
  }

  ngOnDestroy() { clearInterval(this.reloj); }

  recargar() {
    this.api.historial(Number(this.id())).subscribe((m) => this.mensajes.set(m));
  }

  responder() {
    if (!this.texto.trim()) return;
    this.enviando.set(true);
    this.error.set('');
    this.api.responder(Number(this.id()), this.texto).subscribe({
      next: () => { this.texto = ''; this.enviando.set(false); this.recargar(); },
      error: () => {
        this.enviando.set(false);
        // Aquí SÍ se dice: hay una persona delante esperando saber si su
        // mensaje salió. Un fallo silencioso le hace creer que sí.
        this.error.set('No se pudo enviar. El canal lo rechazó.');
      },
    });
  }

  devolver() {
    this.api.devolverAlBot(Number(this.id())).subscribe();
  }

  olvidar() {
    if (!this.confirmando()) {
      this.confirmando.set(true);
      setTimeout(() => this.confirmando.set(false), 5000);
      return;
    }
    this.confirmando.set(false);
    this.api.olvidarContacto(Number(this.id())).subscribe((r) => {
      this.olvidado.set(`${r.conversaciones} conversaciones y ${r.mensajes} mensajes`);
      this.mensajes.set([]);
    });
  }
}
