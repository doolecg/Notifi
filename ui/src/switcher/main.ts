import { mount } from 'svelte';
import Switcher from './Switcher.svelte';
import '../app.css';

export default mount(Switcher, { target: document.getElementById('app')! });
