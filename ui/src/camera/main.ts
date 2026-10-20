import { mount } from 'svelte';
import '../app.css';
import '../lib/fill.css';
import Camera from './Camera.svelte';

export default mount(Camera, { target: document.getElementById('app')! });
