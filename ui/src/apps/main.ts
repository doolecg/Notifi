import { mount } from 'svelte';
import Apps from './Apps.svelte';
import '../app.css';

export default mount(Apps, { target: document.getElementById('app')! });
