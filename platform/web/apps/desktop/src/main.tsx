import { render } from "solid-js/web";
import "@fontsource/fira-sans/400.css";
import "@fontsource/fira-sans/500.css";
import "@fontsource/fira-sans/600.css";
import "@fontsource/fira-code/400.css";
import "@businex/ui/styles.css";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("root element missing");
render(() => <App />, root);
