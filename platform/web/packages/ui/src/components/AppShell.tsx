import { For, type Component, type JSX } from "solid-js";

export interface NavItem {
  label: string;
  href: string;
}

export interface AppShellProps {
  brand: string;
  nav: NavItem[];
  current: string;
  topbar?: JSX.Element;
  children?: JSX.Element;
}

/** Business desktop chrome: dark command rail with an inverted active pill,
 *  top bar for company switching and the user menu, content below. */
export const AppShell: Component<AppShellProps> = (props) => {
  return (
    <div class="bx-shell">
      <nav class="bx-rail" aria-label="Main">
        <span class="bx-rail__brand">{props.brand}</span>
        <For each={props.nav}>
          {(item) => (
            <a
              class="bx-rail__link"
              href={item.href}
              aria-current={item.href === props.current ? "page" : undefined}
            >
              {item.label}
            </a>
          )}
        </For>
      </nav>
      <div class="bx-main">
        <header class="bx-topbar">{props.topbar}</header>
        <main class="bx-content">{props.children}</main>
      </div>
    </div>
  );
};
