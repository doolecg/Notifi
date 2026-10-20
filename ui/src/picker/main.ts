import { mount } from 'svelte';
import Picker from './Picker.svelte';
import '../app.css';
import '../lib/fill.css';

export default mount(Picker, { target: document.getElementById('app')! });
