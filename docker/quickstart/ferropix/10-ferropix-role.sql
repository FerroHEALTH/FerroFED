-- SPDX-FileCopyrightText: Cadasto B.V.
-- SPDX-License-Identifier: BUSL-1.1
--
-- The restricted role FerroPIX connects as: CREATE on its database and
-- nothing else, as FerroPIX's own quickstart creates it. A development
-- password, NOT for anything real.
CREATE ROLE ferropix LOGIN PASSWORD 'ferropix_example';
GRANT CREATE ON DATABASE ferropix TO ferropix;
