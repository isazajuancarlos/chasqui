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
import { MatSelectModule } from '@angular/material/select';
import { Api, Constancia, Consulta, Cuenta, Documento, Flujo, ModeloConfig, Paso, Pregunta } from './api';

const ETIQUETAS: Record<string, string> = {
  'guion.aviso_privacidad': 'Aviso de privacidad (sale en el primer mensaje de cada conversación)',
  'guion.sin_respuesta': 'Cuando el bot no sabe',
  'guion.escalando': 'Cuando pasa con una persona',
  'guion.palabras_escalado': 'Frases con las que piden una persona (una por línea)',
  'guion.palabras_cancelar':
    'Frases para salirse de un flujo de varios pasos (una por línea). ' +
    'Solo cuentan DENTRO de un flujo: fuera, «cancelar» es una palabra más.',
  'guion.cancelado': 'Cuando se salen de un flujo',
  'guion.aviso_flujo':
    'Se añade a la primera pregunta de un flujo, para que sepan que pueden salir. ' +
    'Vacío lo apaga.',
  'retencion.dias':
    'Cuántos días se guardan las conversaciones (se borran solas al vencer). ' +
    'Este plazo lo decide usted: es el responsable del tratamiento.',
};

@Component({
  selector: 'app-configurar',
  imports: [FormsModule, MatButtonModule, MatCardModule, MatFormFieldModule,
            MatInputModule, MatSelectModule, MatSlideToggleModule],
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

    <h2>Flujos de varios pasos</h2>
    <p class="disparadores">
      Para tomar un pedido o agendar una cita: el bot pregunta un dato por
      mensaje y al final arma el resultado, lo guarda en
      <strong>Pedidos</strong> y llama a una persona para que lo despache.
    </p>
    @for (f of flujos(); track f.id) {
      <mat-card class="fila">
        <mat-card-content>
          <div class="consulta">
            <div>
              <strong>{{ f.titulo }}</strong>
              <p class="disparadores">Arranca con: {{ f.disparadores.join(' · ') }}</p>
            </div>
            <mat-slide-toggle [checked]="f.activo"
                              (change)="activarFlujo(f, $event.checked)">
              Activo
            </mat-slide-toggle>
          </div>
          <ol class="pasos">
            @for (p of f.pasos; track $index) {
              <li>
                <strong>{{ p.clave }}</strong>
                <span class="tipo">{{ p.tipo_dato ?? 'texto libre' }}</span>
                — {{ p.pregunta }}
              </li>
            }
          </ol>
          <p class="disparadores">Al terminar dice: «{{ f.confirmacion }}»</p>
          <button mat-button (click)="editarFlujo(f)">Editar</button>
          <button mat-button (click)="borrarFlujo(f)">Borrar</button>
        </mat-card-content>
      </mat-card>
    }
    @if (flujos().length === 0) { <p class="vacio">Ninguno todavía.</p> }

    <mat-card class="fila">
      <mat-card-content>
        <h3>{{ editando() ? 'Editar «' + nuevoFlujo.titulo + '»' : 'Nuevo flujo' }}</h3>
        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Nombre del flujo</mat-label>
          <input matInput [(ngModel)]="nuevoFlujo.titulo" name="flujo_titulo" />
        </mat-form-field>
        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Frases que lo arrancan, una por línea</mat-label>
          <textarea matInput rows="2" [(ngModel)]="nuevoFlujo.disparadores"
                    name="flujo_disparadores"></textarea>
        </mat-form-field>

        <p class="disparadores">Los pasos, en el orden en que se preguntan:</p>
        @for (p of nuevoFlujo.pasos; track $index; let i = $index) {
          <div class="paso">
            <mat-form-field appearance="outline" class="clave">
              <mat-label>Nombre del dato</mat-label>
              <input matInput [(ngModel)]="p.clave" [name]="'clave' + i" />
            </mat-form-field>
            <mat-form-field appearance="outline" class="pregunta">
              <mat-label>Qué se pregunta</mat-label>
              <input matInput [(ngModel)]="p.pregunta" [name]="'pregunta' + i" />
            </mat-form-field>
            <mat-form-field appearance="outline" class="tipo-campo">
              <mat-label>Tipo</mat-label>
              <mat-select [(ngModel)]="p.tipo_dato" [name]="'tipo' + i">
                <mat-option [value]="null">texto libre</mat-option>
                <mat-option value="numero">número</mat-option>
                <mat-option value="correo">correo</mat-option>
                <mat-option value="codigo">código</mat-option>
              </mat-select>
            </mat-form-field>
            <button mat-button (click)="quitarPaso(i)" [disabled]="nuevoFlujo.pasos.length === 1">
              Quitar
            </button>
          </div>
        }
        <button mat-button (click)="anadirPaso()">Añadir paso</button>

        <mat-form-field appearance="outline" class="ancho">
          <mat-label>Qué se dice al terminar. Use {{ '{' }}nombre_del_dato{{ '}' }} para
            repetir lo que contestaron</mat-label>
          <textarea matInput rows="2" [(ngModel)]="nuevoFlujo.confirmacion"
                    name="flujo_confirmacion"></textarea>
        </mat-form-field>

        @if (errorFlujo()) { <p class="alerta">{{ errorFlujo() }}</p> }
        <button mat-flat-button (click)="guardarFlujo()">
          {{ editando() ? 'Guardar los cambios' : 'Crear el flujo' }}
        </button>
        @if (editando()) {
          <button mat-button (click)="limpiarFlujo()">Cancelar</button>
        }
      </mat-card-content>
    </mat-card>

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
    h3 { font-size: 1rem; margin: 0 0 .5rem; }
    .pasos { margin: .5rem 0; padding-left: 1.2rem; }
    .pasos li { margin: .2rem 0; }
    .tipo { font-size: .75rem; color: #666; border: 1px solid #ddd; border-radius: 3px;
            padding: 0 .3rem; margin-left: .3rem; }
    .paso { display: flex; gap: .5rem; align-items: baseline; flex-wrap: wrap; }
    .paso .clave { flex: 1 1 8rem; }
    .paso .pregunta { flex: 3 1 16rem; }
    .paso .tipo-campo { flex: 1 1 8rem; }
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

  /** Todos, encendidos y apagados: el panel tiene que poder volver a encender
   *  uno que apagó. Cada flujo trae su propio `activo` del servidor. */
  flujos = signal<Flujo[]>([]);
  nuevoFlujo = this.flujoEnBlanco();
  editando = signal(false);
  errorFlujo = signal('');

  ngOnInit() {
    this.api.ajustes().subscribe((a) => this.ajustes.set(a));
    this.api.preguntas().subscribe((p) => this.preguntas.set(p));
    this.api.consultas().subscribe((c) => this.consultas.set(c));
    this.api.cuentas().subscribe((c) => this.cuentas.set(c));
    this.api.supresiones().subscribe((s) => this.supresiones.set(s));
    this.api.documentos().subscribe((d) => this.documentos.set(d));
    this.api.modelo().subscribe((m) => this.modelo.set(m));
    this.recargarFlujos();
  }

  private recargarFlujos() {
    this.api.flujos().subscribe((f) => this.flujos.set(f));
  }

  private flujoEnBlanco() {
    return {
      titulo: '',
      disparadores: '',
      confirmacion: '',
      pasos: [{ clave: '', pregunta: '', tipo_dato: null as string | null }] as Paso[],
    };
  }

  anadirPaso() {
    this.nuevoFlujo.pasos = [
      ...this.nuevoFlujo.pasos,
      { clave: '', pregunta: '', tipo_dato: null },
    ];
  }

  quitarPaso(i: number) {
    // Nunca se queda sin ninguno: un flujo sin pasos no recoge nada, y el
    // servidor lo rechazaría. Mejor no dejar llegar ahí.
    if (this.nuevoFlujo.pasos.length <= 1) return;
    this.nuevoFlujo.pasos = this.nuevoFlujo.pasos.filter((_, j) => j !== i);
  }

  editarFlujo(f: Flujo) {
    // Se carga por TÍTULO: guardar con el mismo título reemplaza el flujo entero
    // en el servidor. Cambiarlo aquí crea uno nuevo y deja el viejo, que es lo
    // que se espera al renombrar algo... y por eso se avisa al guardar.
    this.nuevoFlujo = {
      titulo: f.titulo,
      disparadores: f.disparadores.join('\n'),
      confirmacion: f.confirmacion,
      pasos: f.pasos.map((p) => ({ ...p })),
    };
    this.editando.set(true);
    this.errorFlujo.set('');
  }

  limpiarFlujo() {
    this.nuevoFlujo = this.flujoEnBlanco();
    this.editando.set(false);
    this.errorFlujo.set('');
  }

  /**
   * Las mismas comprobaciones que hace el dominio, para decirlas sin ir y
   * volver. **No sustituyen a las del servidor**: esto es una cortesía, y el
   * servidor no puede fiarse de que se haya hecho.
   */
  private queLeFalta(disparadores: string[]): string {
    if (!this.nuevoFlujo.titulo.trim()) return 'El flujo necesita un nombre.';
    if (disparadores.length === 0) {
      return 'Sin frases que lo arranquen, el flujo no se activaría nunca.';
    }
    const claves = this.nuevoFlujo.pasos.map((p) => p.clave.trim());
    if (claves.some((c) => !c)) return 'Cada paso necesita un nombre para el dato.';
    if (this.nuevoFlujo.pasos.some((p) => !p.pregunta.trim())) {
      return 'Cada paso necesita una pregunta: si no, el bot esperaría callado.';
    }
    const repetida = claves.find((c, i) => claves.indexOf(c) !== i);
    if (repetida) {
      return `«${repetida}» está dos veces: el segundo paso borraría lo que contestó el primero.`;
    }
    // Las llaves de la confirmación tienen que ser pasos. Es el error que, si
    // pasara, el cliente vería como un hueco justo al confirmar su pedido.
    const usadas = [...this.nuevoFlujo.confirmacion.matchAll(/{([^}]*)}/g)].map((m) => m[1]);
    const suelta = usadas.find((u) => !claves.includes(u));
    if (suelta !== undefined) {
      return `La confirmación usa {${suelta}}, que no es ninguno de los pasos.`;
    }
    return '';
  }

  guardarFlujo() {
    const disparadores = this.nuevoFlujo.disparadores
      .split('\n').map((d) => d.trim()).filter((d) => d.length > 0);

    const falta = this.queLeFalta(disparadores);
    if (falta) {
      this.errorFlujo.set(falta);
      return;
    }
    this.errorFlujo.set('');

    this.api.guardarFlujo({
      titulo: this.nuevoFlujo.titulo.trim(),
      disparadores,
      confirmacion: this.nuevoFlujo.confirmacion,
      pasos: this.nuevoFlujo.pasos.map((p) => ({
        clave: p.clave.trim(),
        pregunta: p.pregunta.trim(),
        tipo_dato: p.tipo_dato || null,
      })),
    }).subscribe({
      next: () => { this.limpiarFlujo(); this.recargarFlujos(); },
      // El servidor dice QUÉ está mal, y se enseña tal cual: es lo que el
      // cliente acaba de escribir, no un detalle de dentro.
      error: (e) => this.errorFlujo.set(e?.error?.error ?? 'No se pudo guardar el flujo.'),
    });
  }

  activarFlujo(f: Flujo, activo: boolean) {
    this.api.activarFlujo(f.id, activo).subscribe(() => {
      this.flujos.update((l) => l.map((x) => (x.id === f.id ? { ...x, activo } : x)));
    });
  }

  borrarFlujo(f: Flujo) {
    this.api.borrarFlujo(f.id).subscribe(() => {
      this.recargarFlujos();
      if (this.nuevoFlujo.titulo === f.titulo) this.limpiarFlujo();
    });
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
