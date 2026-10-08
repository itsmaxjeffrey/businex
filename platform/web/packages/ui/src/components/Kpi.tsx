import { Show, type Component } from "solid-js";

export interface KpiProps {
  label: string;
  value: string | number;
  unit?: string;
}

/** Metric with mixed-size number/unit: the number is what users scan. */
export const Kpi: Component<KpiProps> = (props) => {
  return (
    <div class="bx-kpi">
      <span class="bx-kpi__label">{props.label}</span>
      <p class="bx-kpi__value">
        <span class="bx-kpi__number">{props.value}</span>
        <Show when={props.unit}>
          <span class="bx-kpi__unit">{props.unit}</span>
        </Show>
      </p>
    </div>
  );
};
