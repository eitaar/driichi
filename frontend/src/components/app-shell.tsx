import type { MouseEvent, ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { House } from "@phosphor-icons/react/dist/csr/House";
import { ChartLine } from "@phosphor-icons/react/dist/csr/ChartLine";
import { FilmStrip } from "@phosphor-icons/react/dist/csr/FilmStrip";
import { SignOut } from "@phosphor-icons/react/dist/csr/SignOut";
import { navigate } from "../routes";
import { signOutAdmin } from "../admin-session";
import { Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent, SidebarHeader, SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger, useSidebar } from "./ui/sidebar";
import { buttonVariants } from "./ui/button";
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "./ui/breadcrumb";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { ThemeToggle } from "./theme-toggle";

function follow(event: MouseEvent<HTMLAnchorElement>, href: string) {
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return false;
  event.preventDefault(); navigate(href); return true;
}

function AdminNavigation({ section }: { section: string }) {
  const { setOpenMobile } = useSidebar();
  const client = useQueryClient();
  return <Sidebar aria-label="Admin navigation" role="complementary"><SidebarHeader><a href="/" onClick={event => { if (follow(event, "/")) setOpenMobile(false); }}>Double Riichi</a></SidebarHeader><SidebarContent>
    <SidebarGroup><SidebarGroupContent><nav aria-label="Admin"><SidebarMenu>{[["Rooms", "/admin"], ["Benchmarks", "/admin/benchmark"], ["Replays", "/admin/replays"]].map(([label, href]) => <SidebarMenuItem key={href}><SidebarMenuButton isActive={section === label} render={<a href={href} onClick={event => { if (follow(event, href)) setOpenMobile(false); }} />}>{label === "Rooms" ? <House aria-hidden="true" /> : label === "Benchmarks" ? <ChartLine aria-hidden="true" /> : <FilmStrip aria-hidden="true" />}<span>{label}</span></SidebarMenuButton></SidebarMenuItem>)}</SidebarMenu></nav></SidebarGroupContent></SidebarGroup>
  </SidebarContent><SidebarFooter className="mt-auto shrink-0 border-t p-3 pb-[max(.75rem,env(safe-area-inset-bottom))]"><SidebarMenu><SidebarMenuItem><Tooltip><TooltipTrigger render={<SidebarMenuButton className="size-11 justify-center" aria-label="Sign out" onClick={() => signOutAdmin(client)} />}><SignOut aria-hidden="true" /></TooltipTrigger><TooltipContent>Sign out</TooltipContent></Tooltip></SidebarMenuItem></SidebarMenu></SidebarFooter></Sidebar>;
}

export function AppShell({ variant, children }: { variant: "public" | "admin" | "table"; children: ReactNode }) {
  // Table surfaces retain their existing compact HUD; persistent chrome must not resize the canvas.
  if (variant === "table") return <>{children}</>;
  if (variant === "public") return <><header className="flex min-h-18 flex-wrap items-center justify-between gap-3 border-b bg-background px-6 py-3 text-foreground sm:px-12"><a className="font-semibold" href="/" onClick={event => follow(event, "/")}>DOUBLE RIICHI</a><nav className="ml-auto flex min-w-0 flex-wrap items-center gap-3" aria-label="Primary navigation"><a className={buttonVariants({ variant: "ghost" })} href="/admin/login" onClick={event => follow(event, "/admin/login")}>Admin sign in</a><ThemeToggle /></nav></header>{children}</>;
  const section = window.location.pathname.startsWith("/admin/benchmark") ? "Benchmarks" : window.location.pathname.startsWith("/admin/replays") ? "Replays" : "Rooms";
  return <SidebarProvider><AdminNavigation section={section} /><SidebarInset className="min-w-0"><header className="flex flex-wrap items-center gap-3 border-b p-3"><SidebarTrigger aria-label="Toggle navigation" /><Breadcrumb><BreadcrumbList><BreadcrumbItem><BreadcrumbLink href="/admin" onClick={event => follow(event, "/admin")}>Admin</BreadcrumbLink></BreadcrumbItem><BreadcrumbSeparator /><BreadcrumbItem><BreadcrumbPage>{section}</BreadcrumbPage></BreadcrumbItem></BreadcrumbList></Breadcrumb><div className="ml-auto"><ThemeToggle /></div></header>{children}</SidebarInset></SidebarProvider>;
}
