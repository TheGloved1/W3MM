import Body from "./table-body.svelte";
import Cell from "./table-cell.svelte";
import Head from "./table-head.svelte";
import Header from "./table-header.svelte";
import Row from "./table-row.svelte";

export {
	Body,
	Cell,
	Head,
	Header,
	Row,
	//
	Body as TableBody,
	Cell as TableCell,
	Head as TableHead,
	Header as TableHeader,
	Row as TableRow,
};

export type { TableBodyProps } from "./table-body.svelte";
export type { TableCellProps } from "./table-cell.svelte";
export type { TableHeadProps } from "./table-head.svelte";
export type { TableHeaderProps } from "./table-header.svelte";
export type { TableRowProps } from "./table-row.svelte";
