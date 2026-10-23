# The find.

search-title = Find
search-subtitle = What the find matched for “{ $words }”.
search-empty-subtitle = Type an organisation, a node, a creating system or a stored query in the find.
search-kind-organisations = Organisations
search-kind-nodes = Nodes
search-kind-queries = Stored queries
search-organisation-line = { $nodes ->
    [one] { $nodes } node
   *[other] { $nodes } nodes
} · { $state }
search-node-line = Node of { $organisation } · { $state }
search-creating-system-line = A creating system: its follow-ups go to this node of { $organisation }
search-query-line = { $count ->
    [one] { $count } version
   *[other] { $count } versions
}
search-nothing = Nothing found for “{ $words }”.
search-nothing-line = The find takes organisations, nodes, creating systems and stored queries, never a patient identifier.
search-to-query = Ask the federation in the query console
search-all-queries = Every stored query
