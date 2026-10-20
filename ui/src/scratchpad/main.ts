import { mount } from 'svelte';
import Scratchpad from './Scratchpad.svelte';
import '../app.css';
import '../lib/fill.css';

export default mount(Scratchpad, { target: document.getElementById('app')! });
