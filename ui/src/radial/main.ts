import { mount } from 'svelte';
import Radial from './Radial.svelte';
import '../app.css';
import '../lib/fill.css';

export default mount(Radial, { target: document.getElementById('app')! });
