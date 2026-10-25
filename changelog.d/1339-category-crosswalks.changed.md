- `eehrxf`'s `Category::crosswalk`, which answered one crosswalk or none, is
  `Category::crosswalks`, which answers the crosswalks of a category's
  datasets in the order of `Category::roots`: two for imaging, one for every
  other category. A caller of `crosswalk()` takes the first of
  `crosswalks()` (#1339).
