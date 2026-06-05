import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');

if (!target) {
  throw new Error('Unable to mount app: missing #app element.');
}

mount(App, { target });
