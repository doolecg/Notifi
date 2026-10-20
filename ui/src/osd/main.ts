import { mount } from 'svelte';
import Osd from './Osd.svelte';
import '../app.css';

export default mount(Osd, { target: document.getElementById('app')! });
