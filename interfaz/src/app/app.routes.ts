// SPDX-License-Identifier: LicenseRef-Propietario-IsazaArenas
// Copyright (c) 2026 Juan Carlos Isaza Arenas
import { Routes } from '@angular/router';
import { inject } from '@angular/core';
import { Router } from '@angular/router';
import { map, catchError, of } from 'rxjs';
import { Api } from './api';

/// Sin sesión no se entra. El guardián pregunta al servidor en vez de mirar una
/// bandera del navegador: la sesión la decide el servidor, y una bandera local
/// solo dice lo que este navegador cree.
const conSesion = () => {
  const api = inject(Api);
  const router = inject(Router);
  return api.quienSoy().pipe(
    map((yo) => { api.yo.set(yo); return true; }),
    catchError(() => of(router.createUrlTree(['/entrar']))),
  );
};

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'cola' },
  { path: 'entrar', loadComponent: () => import('./entrar').then((m) => m.Entrar) },
  {
    path: 'cola',
    canActivate: [conSesion],
    loadComponent: () => import('./cola').then((m) => m.Cola),
  },
  {
    path: 'conversacion/:id',
    canActivate: [conSesion],
    loadComponent: () => import('./conversacion').then((m) => m.ConversacionVista),
  },
  {
    path: 'pedidos',
    canActivate: [conSesion],
    loadComponent: () => import('./pedidos').then((m) => m.Pedidos),
  },
  {
    path: 'configurar',
    canActivate: [conSesion],
    loadComponent: () => import('./configurar').then((m) => m.Configurar),
  },
  { path: '**', redirectTo: 'cola' },
];
