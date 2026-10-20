import { mount } from 'svelte';
import Radial from './Radial.svelte';
import '../app.css';

export default mount(Radial, { target: document.getElementById('app')! });
