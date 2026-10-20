import { mount } from 'svelte';
import '../app.css';
import '../lib/fill.css';

// The trimmer is a normal window; loading it apart keeps the bar's transparent page style off it.
const view = location.hash === '#trim' ? (await import('./Trim.svelte')).default : (await import('./Recording.svelte')).default;

export default mount(view, { target: document.getElementById('app')! });
