import Content from "./card-content.svelte";
import Description from "./card-description.svelte";
import Header from "./card-header.svelte";
import Title from "./card-title.svelte";
import Root from "./card.svelte";

export {
	Root,
	Header,
	Title,
	Description,
	Content,
	//
	Root as Card,
	Header as CardHeader,
	Title as CardTitle,
	Description as CardDescription,
	Content as CardContent,
};

export type { CardProps } from "./card.svelte";
export type { CardHeaderProps } from "./card-header.svelte";
export type { CardTitleProps } from "./card-title.svelte";
export type { CardDescriptionProps } from "./card-description.svelte";
export type { CardContentProps } from "./card-content.svelte";
