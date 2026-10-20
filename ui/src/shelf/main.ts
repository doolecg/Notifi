import { mount } from 'svelte';
import Shelf from './Shelf.svelte';
import '../app.css';

export default mount(Shelf, { target: document.getElementById('app')! });
