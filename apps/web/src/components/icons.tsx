import React from "react";

type P = { size?: number; className?: string };
const base = (size: number) => ({
  width: size, height: size, viewBox: "0 0 24 24", fill: "none",
  stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const,
});

export const IconDashboard = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><rect x="3" y="3" width="7" height="9" rx="2" /><rect x="14" y="3" width="7" height="5" rx="2" /><rect x="14" y="12" width="7" height="9" rx="2" /><rect x="3" y="16" width="7" height="5" rx="2" /></svg>
);
export const IconCrm = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><circle cx="9" cy="8" r="3.5" /><path d="M3.5 20c.8-3.2 3-5 5.5-5s4.7 1.8 5.5 5" /><circle cx="17" cy="9" r="2.5" /><path d="M15.5 14.5c2.2.2 4 1.7 4.8 4.5" /></svg>
);
export const IconProjects = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M7 9h6M7 13h10M7 17h4" /></svg>
);
export const IconDocs = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M6 3h8l4 4v14H6z" /><path d="M14 3v4h4" /><path d="M9 12h6M9 16h6" /></svg>
);
export const IconCalendar = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><rect x="3" y="5" width="18" height="16" rx="2.5" /><path d="M8 3v4M16 3v4M3 10h18" /></svg>
);
export const IconInvoice = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M5 3h14v18l-3-2-2 2-2-2-2 2-2-2-3 2z" /><path d="M9 8h6M9 12h6" /></svg>
);
export const IconChannels = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M4 6h16M4 12h10M4 18h13" /><circle cx="19" cy="12" r="2" /></svg>
);
export const IconAgent = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><rect x="5" y="8" width="14" height="11" rx="3" /><path d="M12 8V4M9 4h6" /><circle cx="9.5" cy="13" r="1" /><circle cx="14.5" cy="13" r="1" /><path d="M9.5 16.5h5" /></svg>
);
export const IconTerminal = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M7 9l3 3-3 3M13 15h4" /></svg>
);
export const IconSettings = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2" /></svg>
);
export const IconSearch = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
);
export const IconSun = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><circle cx="12" cy="12" r="4" /><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5 5l1.5 1.5M17.5 17.5L19 19M19 5l-1.5 1.5M6.5 17.5L5 19" /></svg>
);
export const IconMoon = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M20 13.5A8 8 0 1 1 10.5 4a6.5 6.5 0 0 0 9.5 9.5z" /></svg>
);
export const IconPlus = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M12 5v14M5 12h14" /></svg>
);
export const IconSend = ({ size = 20, className }: P) => (
  <svg {...base(size)} className={className}><path d="M4 12l16-7-6 16-2.5-6.5z" /></svg>
);
