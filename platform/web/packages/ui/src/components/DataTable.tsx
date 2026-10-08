import { For, Show, type Component, type JSX } from "solid-js";
import { EmptyState } from "./EmptyState";

export type Cell = string | number | JSX.Element;

export interface Column {
  key: string;
  header: string;
  numeric?: boolean;
  code?: boolean;
}

export interface Row {
  id: string;
  cells: Record<string, Cell>;
}

export interface DataTableProps {
  caption: string;
  columns: Column[];
  rows: Row[];
  emptyTitle?: string;
  emptyBody?: string;
  emptyAction?: JSX.Element;
}

/** Semantic table with a caption and scoped headers; numbers right-aligned,
 *  identifiers in the code face. Rows fall back to an empty state. */
export const DataTable: Component<DataTableProps> = (props) => {
  return (
    <Show
      when={props.rows.length > 0}
      fallback={
        <EmptyState
          title={props.emptyTitle ?? "Nothing here yet"}
          body={props.emptyBody ?? "Records you add will appear here."}
          action={props.emptyAction}
        />
      }
    >
      <div class="bx-table-wrap">
        <table class="bx-table">
          <caption>{props.caption}</caption>
          <thead>
            <tr>
              <For each={props.columns}>
                {(column) => (
                  <th
                    scope="col"
                    classList={{ "bx-num": column.numeric ?? false }}
                  >
                    {column.header}
                  </th>
                )}
              </For>
            </tr>
          </thead>
          <tbody>
            <For each={props.rows}>
              {(row) => (
                <tr>
                  <For each={props.columns}>
                    {(column) => (
                      <td
                        classList={{
                          "bx-num": column.numeric ?? false,
                          "bx-code": column.code ?? false
                        }}
                      >
                        {row.cells[column.key]}
                      </td>
                    )}
                  </For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  );
};
