import { mount } from 'svelte';
import Scratchpad from './Scratchpad.svelte';
import '../app.css';

export default mount(Scratchpad, { target: document.getElementById('app')! });
