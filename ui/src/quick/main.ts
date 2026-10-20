import { mount } from 'svelte';
import Quick from './Quick.svelte';
import '../app.css';

export default mount(Quick, { target: document.getElementById('app')! });
