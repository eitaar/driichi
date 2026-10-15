import type { MouseEvent, ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { navigate } from "../routes";
import { signOutAdmin } from "../admin-session";
import { Sidebar, SidebarContent, SidebarHeader, SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger, useSidebar } from "./ui/sidebar";
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "./ui/breadcrumb";

function follow(event: MouseEvent<HTMLAnchorElement>, href: string) {
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return false;
  event.preventDefault(); navigate(href); return true;
}

function AdminNavigation() {
  const { setOpenMobile } = useSidebar();
  const client = useQueryClient();
  return <Sidebar aria-label="Admin navigation"><SidebarHeader><a href="/" onClick={event => follow(event, "/")}>Double Riichi</a></SidebarHeader><SidebarContent>
    <nav aria-label="Admin"><SidebarMenu>{[["Rooms", "/admin"], ["Benchmarks", "/admin/benchmark"], ["Replays", "/admin/replays"]].map(([label, href]) => <SidebarMenuItem key={href}><SidebarMenuButton render={<a href={href} onClick={event => { if (follow(event, href)) setOpenMobile(false); }} />}>{label}</SidebarMenuButton></SidebarMenuItem>)}</SidebarMenu></nav>
    <SidebarMenu><SidebarMenuItem><SidebarMenuButton onClick={() => signOutAdmin(client)}>Sign out</SidebarMenuButton></SidebarMenuItem></SidebarMenu>
  </SidebarContent></Sidebar>;
}

export function AppShell({ variant, children }: { variant: "public" | "admin" | "table"; children: ReactNode }) {
  // Table surfaces retain their existing compact HUD; persistent chrome must not resize the canvas.
  if (variant === "table") return <>{children}</>;
  if (variant === "public") return <><header className="topbar"><a className="brand" href="/" onClick={event => follow(event, "/")}>DOUBLE RIICHI</a><nav aria-label="Primary navigation"><a className="nav-link" href="/admin/login" onClick={event => follow(event, "/admin/login")}>Admin sign in</a></nav></header>{children}</>;
  const section = window.location.pathname.startsWith("/admin/benchmark") ? "Benchmarks" : window.location.pathname.startsWith("/admin/replays") ? "Replays" : "Rooms";
  return <SidebarProvider><AdminNavigation /><SidebarInset className="min-w-0"><header className="flex items-center gap-3 border-b p-3"><SidebarTrigger aria-label="Toggle navigation" /><Breadcrumb><BreadcrumbList><BreadcrumbItem><BreadcrumbLink href="/admin" onClick={event => follow(event, "/admin")}>Admin</BreadcrumbLink></BreadcrumbItem><BreadcrumbSeparator /><BreadcrumbItem><BreadcrumbPage>{section}</BreadcrumbPage></BreadcrumbItem></BreadcrumbList></Breadcrumb></header>{children}</SidebarInset></SidebarProvider>;
}
