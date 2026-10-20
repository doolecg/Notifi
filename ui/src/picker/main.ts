import { mount } from 'svelte';
import Picker from './Picker.svelte';
import '../app.css';

export default mount(Picker, { target: document.getElementById('app')! });
